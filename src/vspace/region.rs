use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use super::{KernelRetypeFanOutLimit, NumPages, VSpaceError};
use crate::arch::cap::{page_state, Page};
use crate::arch::PageBits;
use crate::cap::{
    memory_kind, role, CNodeRole, CNodeSlots, Cap, CapRange, InternalASID, LocalCNode,
    LocalCNodeSlots, LocalCap, MemoryKind, RetypeError, Untyped, WCNodeSlots, WUntyped,
    WeakCapRange, WeakMemoryKind,
};

use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;

pub trait SharedStatus: private::SealedSharedStatus {}

pub mod shared_status {
    use super::SharedStatus;

    pub struct Shared;
    impl SharedStatus for Shared {}

    pub struct Exclusive;
    impl SharedStatus for Exclusive {}
}

mod private {
    use super::shared_status::{Exclusive, Shared};
    pub trait SealedSharedStatus {}
    impl SealedSharedStatus for Shared {}
    impl SealedSharedStatus for Exclusive {}
}

/// A `1 << SizeBits` bytes region of unmapped memory. It can be
/// shared or owned exclusively. The ramifications of its shared
/// status are described more completely in the `mapped_shared_region`
/// function description.
pub struct UnmappedMemoryRegion<SizeBits: Unsigned, SS: SharedStatus>
where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(super) caps: CapRange<Page<page_state::Unmapped>, role::Local, NumPages<SizeBits>>,
    pub(super) kind: WeakMemoryKind,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl LocalCap<Page<page_state::Unmapped>> {
    /// N.B. until MemoryKind tracking is added to Page, this is a lossy conversion
    /// that will assume the Page was for General memory
    pub(crate) fn to_region(self) -> UnmappedMemoryRegion<PageBits, shared_status::Exclusive> {
        UnmappedMemoryRegion::unchecked_new(self.cptr, WeakMemoryKind::General)
    }
}

impl<SizeBits: Unsigned, SS: SharedStatus> UnmappedMemoryRegion<SizeBits, SS>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    /// The size of this region in bytes.
    pub const SIZE_BYTES: usize = 1 << SizeBits::USIZE;

    pub fn size_bytes(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub(super) fn unchecked_new(local_page_caps_offset_cptr: usize, kind: WeakMemoryKind) -> Self {
        UnmappedMemoryRegion {
            caps: CapRange::new_phantom(local_page_caps_offset_cptr),
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub(super) fn from_caps(
        caps: CapRange<Page<page_state::Unmapped>, role::Local, NumPages<SizeBits>>,
        kind: WeakMemoryKind,
    ) -> UnmappedMemoryRegion<SizeBits, SS> {
        UnmappedMemoryRegion {
            caps,
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }
}

impl<SizeBits: Unsigned> UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    /// Retype the necessary number of granules into memory
    /// capabilities and return the unmapped region.
    pub fn new(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.retype_pages(slots)?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    pub fn new_device<Role: CNodeRole>(
        ut: LocalCap<Untyped<SizeBits, memory_kind::Device>>,
        slots: CNodeSlots<NumPages<SizeBits>, Role>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.retype_device_pages(slots)?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    /// N.B. until MemoryKind tracking is added to Page, this is a lossy conversion
    /// that will assume the Region was for General memory
    pub(crate) fn to_page(self) -> LocalCap<Page<page_state::Unmapped>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: crate::cap::PhantomCap::phantom_instance(),
            _role: PhantomData,
        }
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion::from_caps(self.caps, self.kind)
    }
}

/// A memory region which is mapped into an address space, meaning it
/// has a virtual address and an associated asid in which that virtual
/// address is valid.
///
/// The distinction between its shared-or-not-shared status is to
/// prevent an unwitting unmap into an `UnmappedMemoryRegion` which
/// loses the sharededness context.
pub struct MappedMemoryRegion<SizeBits: Unsigned, SS: SharedStatus>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(super) caps: CapRange<Page<page_state::Mapped>, role::Local, NumPages<SizeBits>>,
    pub(super) kind: WeakMemoryKind,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<SizeBits: Unsigned, SS: SharedStatus> MappedMemoryRegion<SizeBits, SS>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub const SIZE_BYTES: usize = 1 << SizeBits::USIZE;

    pub fn size(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// mapped memory region, marked as shared.
    pub fn share<CNodeSlotCount: Unsigned>(
        self,
        slots: LocalCNodeSlots<CNodeSlotCount>,
        cnode: &LocalCap<LocalCNode>,
        rights: CapRights,
    ) -> Result<
        (
            UnmappedMemoryRegion<SizeBits, shared_status::Shared>,
            MappedMemoryRegion<SizeBits, shared_status::Shared>,
        ),
        VSpaceError,
    >
    where
        CNodeSlotCount: IsEqual<NumPages<SizeBits>, Output = True>,
    {
        let pages_offset = self.caps.start_cptr;
        let vaddr = self.vaddr();
        let asid = self.asid();
        let slots_offset = slots.cap_data.offset;

        for (slot, page) in slots.iter().zip(self.caps.into_iter()) {
            page.copy(cnode, slot, rights)?;
        }

        Ok((
            UnmappedMemoryRegion::unchecked_new(slots_offset, self.kind),
            MappedMemoryRegion::unchecked_new(pages_offset, vaddr, asid, self.kind),
        ))
    }

    pub(super) fn unchecked_new(
        initial_cptr: usize,
        initial_vaddr: usize,
        asid: InternalASID,
        kind: WeakMemoryKind,
    ) -> MappedMemoryRegion<SizeBits, SS> {
        MappedMemoryRegion {
            caps: CapRange::new(
                initial_cptr,
                Page {
                    state: page_state::Mapped {
                        vaddr: initial_vaddr,
                        asid,
                    },
                },
            ),
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion::unchecked_new(
            self.caps.start_cptr,
            self.vaddr(),
            self.asid(),
            self.kind,
        )
    }

    /// Halve a region into two regions.
    pub fn split(
        self,
    ) -> Result<
        (
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
        ),
        VSpaceError,
    >
    where
        SizeBits: Sub<U1>,
        <SizeBits as Sub<U1>>::Output: Unsigned,
        <SizeBits as Sub<U1>>::Output: IsGreaterOrEqual<U12, Output = True>,
        <SizeBits as Sub<U1>>::Output: Sub<PageBits>,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: Unsigned,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: _Pow,
        Pow<<<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output>: Unsigned,
    {
        let new_region_vaddr = if let Some(vaddr) = 2_usize
            .checked_pow(SizeBits::U32 - 1)
            .and_then(|v| v.checked_add(self.vaddr()))
        {
            vaddr
        } else {
            return Err(VSpaceError::ExceededAvailableAddressSpace);
        };

        let new_offset = self.caps.start_cptr + (self.caps.len() / 2);

        Ok((
            MappedMemoryRegion {
                caps: CapRange::new(
                    self.caps.start_cptr,
                    Page {
                        state: page_state::Mapped {
                            vaddr: self.vaddr(),
                            asid: self.asid(),
                        },
                    },
                ),
                kind: self.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            MappedMemoryRegion {
                caps: CapRange::new(
                    new_offset,
                    Page {
                        state: page_state::Mapped {
                            vaddr: new_region_vaddr,
                            asid: self.asid(),
                        },
                    },
                ),
                kind: self.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
        ))
    }

    /// Splits a range into a specific size and a SizeBits-1 region.
    ///
    /// NB: This function drops on the floor the leftovers between
    /// TargetSize and SizeBits-1 It's only meant to be used to set up
    /// regions for supporting ferros-test.
    ///
    /// Something like:
    /// ```not_rust
    /// SizeBits = 20, TargetSize = 16
    /// [                 20                   ]
    /// [        19        |         19        ]
    /// [        19        | 16 |   dropped    ]
    /// ```
    #[cfg(feature = "test_support")]
    pub fn split_into<TargetSize: Unsigned>(
        self,
    ) -> Result<
        (
            MappedMemoryRegion<TargetSize, SS>,
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
        ),
        VSpaceError,
    >
    where
        TargetSize: IsGreaterOrEqual<PageBits>,
        TargetSize: Sub<PageBits>,
        <TargetSize as Sub<PageBits>>::Output: Unsigned,
        <TargetSize as Sub<PageBits>>::Output: _Pow,
        Pow<<TargetSize as Sub<PageBits>>::Output>: Unsigned,

        SizeBits: Sub<U1>,
        <SizeBits as Sub<U1>>::Output: Unsigned,
        <SizeBits as Sub<U1>>::Output: IsGreaterOrEqual<U12, Output = True>,
        <SizeBits as Sub<U1>>::Output: Sub<PageBits>,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: Unsigned,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: _Pow,
        Pow<<<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output>: Unsigned,
    {
        let (a, b) = self.split()?;

        Ok((
            MappedMemoryRegion {
                caps: CapRange::new(
                    a.caps.start_cptr,
                    Page {
                        state: page_state::Mapped {
                            vaddr: a.vaddr(),
                            asid: a.asid(),
                        },
                    },
                ),
                kind: a.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            b,
        ))
    }
}

pub struct WeakUnmappedMemoryRegion<SS: SharedStatus> {
    caps: WeakCapRange<Page<page_state::Unmapped>, role::Local>,
    kind: WeakMemoryKind,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl WeakUnmappedMemoryRegion<shared_status::Exclusive> {
    pub fn new<MemKind: MemoryKind>(
        untyped: LocalCap<WUntyped<MemKind>>,
        slots: &mut WCNodeSlots,
    ) -> Result<Self, RetypeError> {
        let kind = untyped.cap_data.kind.weaken();
        let size_bits = untyped.size_bits();
        let caps = untyped.retype_pages(slots)?;
        Ok(WeakUnmappedMemoryRegion {
            caps,
            kind,
            size_bits,
            _shared_status: PhantomData,
        })
    }
}
impl<SS: SharedStatus> WeakUnmappedMemoryRegion<SS> {
    pub fn size_bits(&self) -> u8 {
        self.size_bits
    }
    pub fn size_bytes(&self) -> usize {
        2usize.pow(u32::from(self.size_bits))
    }

    pub fn to_shared(self) -> WeakUnmappedMemoryRegion<shared_status::Shared> {
        WeakUnmappedMemoryRegion {
            caps: self.caps,
            kind: self.kind,
            size_bits: self.size_bits,
            _shared_status: PhantomData,
        }
    }
}

pub struct WeakMappedMemoryRegion<SS: SharedStatus> {
    pub(super) caps: WeakCapRange<Page<page_state::Mapped>, role::Local>,
    kind: WeakMemoryKind,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl<SS: SharedStatus> WeakMappedMemoryRegion<SS> {
    pub fn size_bits(&self) -> u8 {
        self.size_bits
    }
    pub fn size_bytes(&self) -> usize {
        2usize.pow(u32::from(self.size_bits))
    }

    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    pub(super) fn unchecked_new(
        initial_cptr: usize,
        initial_vaddr: usize,
        asid: InternalASID,
        kind: WeakMemoryKind,
        size_bits: u8,
    ) -> WeakMappedMemoryRegion<SS> {
        let num_pages = 1 << usize::from(size_bits - PageBits::U8);
        WeakMappedMemoryRegion {
            caps: WeakCapRange::new(
                initial_cptr,
                Page {
                    state: page_state::Mapped {
                        vaddr: initial_vaddr,
                        asid,
                    },
                },
                num_pages,
            ),
            kind,
            size_bits,
            _shared_status: PhantomData,
        }
    }
}
