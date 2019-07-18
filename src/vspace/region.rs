use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use super::{KernelRetypeFanOutLimit, NumPages, VSpaceError};
use crate::arch::cap::{page_state, Page};
use crate::arch::{PageBits, PageBytes};
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
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl LocalCap<Page<page_state::Unmapped>> {
    pub(crate) fn to_region(self) -> UnmappedMemoryRegion<PageBits, shared_status::Exclusive> {
        let caps: CapRange<Page<page_state::Unmapped>, role::Local, U1> = CapRange::new(self.cptr);
        UnmappedMemoryRegion {
            caps,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
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

    pub fn size(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub(super) fn unchecked_new(local_page_caps_offset_cptr: usize) -> Self {
        UnmappedMemoryRegion {
            caps: CapRange::new(local_page_caps_offset_cptr),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub(super) fn from_caps(
        caps: CapRange<Page<page_state::Unmapped>, role::Local, NumPages<SizeBits>>,
    ) -> Self {
        UnmappedMemoryRegion {
            caps,
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
        let page_caps = ut.retype_pages(slots)?;
        Ok(UnmappedMemoryRegion {
            caps: CapRange::new(page_caps.start_cptr),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        })
    }

    pub fn new_device<Role: CNodeRole>(
        ut: LocalCap<Untyped<SizeBits, memory_kind::Device>>,
        slots: CNodeSlots<NumPages<SizeBits>, Role>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let page_caps = ut.retype_device_pages(slots)?;
        Ok(UnmappedMemoryRegion {
            caps: CapRange::new(page_caps.start_cptr),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        })
    }

    pub(crate) fn to_page(self) -> LocalCap<Page<page_state::Unmapped>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: Page {
                state: page_state::Unmapped {},
            },
            _role: PhantomData,
        }
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion {
            caps: self.caps,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }
}

pub(super) struct MappedPageRange<Count: Unsigned> {
    pub(super) initial_cptr: usize,
    initial_vaddr: usize,
    asid: InternalASID,
    _count: PhantomData<Count>,
}

impl<Count: Unsigned> MappedPageRange<Count> {
    fn new(initial_cptr: usize, initial_vaddr: usize, asid: InternalASID) -> Self {
        MappedPageRange {
            initial_cptr,
            initial_vaddr,
            asid,
            _count: PhantomData,
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Cap<Page<page_state::Mapped>, role::Local>> {
        (0..Count::USIZE).map(move |idx| Cap {
            cptr: self.initial_cptr + idx,
            cap_data: Page {
                state: page_state::Mapped {
                    vaddr: self.initial_vaddr + (PageBytes::USIZE * idx),
                    asid: self.asid,
                },
            },
            _role: PhantomData,
        })
    }

    pub fn count(&self) -> usize {
        Count::USIZE
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
    vaddr: usize,
    pub(super) caps: MappedPageRange<NumPages<SizeBits>>,
    asid: InternalASID,
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
        self.vaddr
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
        let pages_offset = self.caps.initial_cptr;
        let vaddr = self.vaddr;
        let asid = self.asid;
        let slots_offset = slots.cap_data.offset;

        for (slot, page) in slots.iter().zip(self.caps.iter()) {
            page.copy(cnode, slot, rights)?;
        }

        Ok((
            UnmappedMemoryRegion::unchecked_new(slots_offset),
            MappedMemoryRegion::unchecked_new(pages_offset, vaddr, asid),
        ))
    }

    pub(super) fn unchecked_new(
        initial_cptr: usize,
        initial_vaddr: usize,
        asid: InternalASID,
    ) -> MappedMemoryRegion<SizeBits, SS> {
        MappedMemoryRegion {
            caps: MappedPageRange::new(initial_cptr, initial_vaddr, asid),
            vaddr: initial_vaddr,
            asid,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion::unchecked_new(self.caps.initial_cptr, self.vaddr, self.asid)
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
            .and_then(|v| v.checked_add(self.vaddr))
        {
            vaddr
        } else {
            return Err(VSpaceError::ExceededAvailableAddressSpace);
        };

        let new_offset = self.caps.initial_cptr + (self.caps.count() / 2);

        Ok((
            MappedMemoryRegion {
                caps: MappedPageRange::new(self.caps.initial_cptr, self.vaddr, self.asid),
                vaddr: self.vaddr,
                asid: self.asid,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            MappedMemoryRegion {
                caps: MappedPageRange::new(new_offset, new_region_vaddr, self.asid),
                vaddr: new_region_vaddr,
                asid: self.asid,
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
                caps: MappedPageRange::new(a.caps.initial_cptr, a.vaddr, a.asid),
                vaddr: a.vaddr,
                asid: a.asid,
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
        //let num_pages = 1 << (untyped.size_bits() - PageBits::U8);
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
