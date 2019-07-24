use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use super::{KernelRetypeFanOutLimit, NumPages, VSpaceError};
use crate::arch::PageBits;
use crate::cap::{
    page_state, role, CNodeRole, CNodeSlotsData, Cap, CapRange, InternalASID, LocalCNode,
    LocalCNodeSlots, LocalCap, MemoryKind, Page, RetypeError, Untyped, WCNodeSlotsData, WUntyped,
    WeakCapRange,
};

use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;
use core::clone::Clone;

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
pub struct UnmappedMemoryRegion<
    SizeBits: Unsigned,
    SS: SharedStatus,
    CapRole: CNodeRole,
    MemKind: MemoryKind,
> where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(super) caps: CapRange<Page<page_state::Unmapped, MemKind>, CapRole, NumPages<SizeBits>>,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<MemKind: MemoryKind> LocalCap<Page<page_state::Unmapped, MemKind>> {
    pub(crate) fn to_region(
        self,
    ) -> UnmappedMemoryRegion<PageBits, shared_status::Exclusive, role::Local, MemKind> {
        UnmappedMemoryRegion::from_caps(CapRange::new(self.cptr, self.cap_data))
    }
}

impl<SizeBits: Unsigned, SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind>
    UnmappedMemoryRegion<SizeBits, SS, CapRole, MemKind>
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

    pub(super) fn from_caps(
        caps: CapRange<Page<page_state::Unmapped, MemKind>, CapRole, NumPages<SizeBits>>,
    ) -> UnmappedMemoryRegion<SizeBits, SS, CapRole, MemKind> {
        UnmappedMemoryRegion {
            caps,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub fn weaken(self) -> WeakUnmappedMemoryRegion<SS, CapRole, MemKind> {
        WeakUnmappedMemoryRegion::try_from_caps(self.caps.weaken(), SizeBits::U8)
            .expect("Type level constraints enforce the page caps to size bits correspondence")
    }
}

impl<SizeBits: Unsigned, CapRole: CNodeRole, MemKind: MemoryKind>
    UnmappedMemoryRegion<SizeBits, shared_status::Exclusive, CapRole, MemKind>
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
        ut: LocalCap<Untyped<SizeBits, MemKind>>,
        slots: LocalCap<CNodeSlotsData<NumPages<SizeBits>, CapRole>>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let page_caps = ut.retype_pages(slots)?;
        Ok(UnmappedMemoryRegion::from_caps(CapRange::new(
            page_caps.start_cptr,
            page_caps.start_cap_data,
        )))
    }

    pub(crate) fn to_page(self) -> LocalCap<Page<page_state::Unmapped, MemKind>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: self.caps.start_cap_data,
            _role: PhantomData,
        }
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(
        self,
    ) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared, CapRole, MemKind> {
        UnmappedMemoryRegion::from_caps(self.caps)
    }
}

/// A memory region which is mapped into an address space, meaning it
/// has a virtual address and an associated asid in which that virtual
/// address is valid.
///
/// The distinction between its shared-or-not-shared status is to
/// prevent an unwitting unmap into an `UnmappedMemoryRegion` which
/// loses the sharededness context.
pub struct MappedMemoryRegion<
    SizeBits: Unsigned,
    SS: SharedStatus,
    CapRole: CNodeRole,
    MemKind: MemoryKind,
> where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(super) caps: CapRange<Page<page_state::Mapped, MemKind>, CapRole, NumPages<SizeBits>>,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<SizeBits: Unsigned, SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind>
    MappedMemoryRegion<SizeBits, SS, CapRole, MemKind>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub const SIZE_BYTES: usize = 1 << SizeBits::USIZE;

    pub fn size_bits(&self) -> u8 {
        SizeBits::U8
    }

    pub fn size_bytes(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    pub(super) fn from_caps(
        caps: CapRange<Page<page_state::Mapped, MemKind>, CapRole, NumPages<SizeBits>>,
    ) -> MappedMemoryRegion<SizeBits, SS, CapRole, MemKind> {
        MappedMemoryRegion {
            caps,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub fn weaken(self) -> WeakMappedMemoryRegion<SS, CapRole, MemKind> {
        WeakMappedMemoryRegion::try_from_caps(self.caps.weaken(), SizeBits::U8)
            .expect("Cap page slots to memory region size invariant maintained by type signature")
    }
    /// N.B. until MemoryKind tracking is added to Page, this is a lossy conversion
    /// that will assume the Region was for General memory
    pub(crate) fn to_page(self) -> LocalCap<Page<page_state::Mapped, MemKind>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: self.caps.start_cap_data,
            _role: PhantomData,
        }
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion::from_caps(CapRange::new(
            self.caps.start_cptr,
            self.caps.start_cap_data.clone(),
        ))
    }

    /// Halve a region into two regions.
    pub fn split(
        self,
    ) -> Result<
        (
            MappedMemoryRegion<op!(SizeBits - U1), SS, CapRole, MemKind>,
            MappedMemoryRegion<op!(SizeBits - U1), SS, CapRole, MemKind>,
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
        let size_bytes = self.size_bytes();
        let new_region_vaddr = if let Some(vaddr) = self.vaddr().checked_add(size_bytes) {
            vaddr
        } else {
            return Err(VSpaceError::ExceededAddressableSpace);
        };

        let new_offset = self.caps.start_cptr + (self.caps.len() / 2);
        let (kind_a, kind_b) = self
            .caps
            .start_cap_data
            .memory_kind
            .halve(size_bytes)
            .ok_or_else(|| VSpaceError::ExceededAddressableSpace)?;
        Ok((
            MappedMemoryRegion {
                caps: CapRange::new(
                    self.caps.start_cptr,
                    Page {
                        state: page_state::Mapped {
                            vaddr: self.vaddr(),
                            asid: self.asid(),
                        },
                        memory_kind: kind_a,
                    },
                ),
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
                        memory_kind: kind_b,
                    },
                ),
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
            MappedMemoryRegion<TargetSize, SS, CapRole, MemKind>,
            MappedMemoryRegion<op!(SizeBits - U1), SS, CapRole, MemKind>,
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
                caps: CapRange::new(a.caps.start_cptr, a.caps.start_cap_data),
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            b,
        ))
    }
}
impl<SizeBits: Unsigned, SS: SharedStatus, MemKind: MemoryKind>
    MappedMemoryRegion<SizeBits, SS, role::Local, MemKind>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// mapped memory region, marked as shared.
    pub fn share<CNodeSlotCount: Unsigned>(
        self,
        // N.B. Likely possible to make this function generic over destination slot Role here
        slots: LocalCNodeSlots<CNodeSlotCount>,
        cnode: &LocalCap<LocalCNode>,
        rights: CapRights,
    ) -> Result<
        (
            UnmappedMemoryRegion<SizeBits, shared_status::Shared, role::Local, MemKind>,
            MappedMemoryRegion<SizeBits, shared_status::Shared, role::Local, MemKind>,
        ),
        VSpaceError,
    >
    where
        CNodeSlotCount: IsEqual<NumPages<SizeBits>, Output = True>,
    {
        let pages_offset = self.caps.start_cptr;
        let slots_offset = slots.cap_data.offset;
        let mapped_cap_data_clone = self.caps.start_cap_data.clone();
        let kind = self.caps.start_cap_data.memory_kind;

        for (slot, page) in slots.iter().zip(self.caps.into_iter()) {
            page.copy(cnode, slot, rights)?;
        }

        Ok((
            UnmappedMemoryRegion::from_caps(CapRange::new(
                slots_offset,
                Page {
                    state: page_state::Unmapped,
                    memory_kind: kind,
                },
            )),
            MappedMemoryRegion::from_caps(CapRange::new(pages_offset, mapped_cap_data_clone)),
        ))
    }
}

pub struct WeakUnmappedMemoryRegion<SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind> {
    pub(super) caps: WeakCapRange<Page<page_state::Unmapped, MemKind>, CapRole>,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl<CapRole: CNodeRole, MemKind: MemoryKind>
    WeakUnmappedMemoryRegion<shared_status::Exclusive, CapRole, MemKind>
{
    pub fn new(
        untyped: LocalCap<WUntyped<MemKind>>,
        slots: &mut LocalCap<WCNodeSlotsData<CapRole>>,
    ) -> Result<Self, RetypeError> {
        let size_bits = untyped.size_bits();
        let caps = untyped.retype_pages(slots)?;
        Ok(WeakUnmappedMemoryRegion {
            caps,
            size_bits,
            _shared_status: PhantomData,
        })
    }
}
impl<SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind>
    WeakUnmappedMemoryRegion<SS, CapRole, MemKind>
{
    pub fn size_bits(&self) -> u8 {
        self.size_bits
    }
    pub fn size_bytes(&self) -> usize {
        2usize.pow(u32::from(self.size_bits))
    }

    pub(super) fn try_from_caps(
        caps: WeakCapRange<Page<page_state::Unmapped, MemKind>, CapRole>,
        size_bits: u8,
    ) -> Result<WeakUnmappedMemoryRegion<SS, CapRole, MemKind>, InvalidSizeBits> {
        if num_pages(size_bits)? != caps.len() {
            return Err(InvalidSizeBits::SizeBitsMismatchPageCapCount);
        }
        Ok(WeakUnmappedMemoryRegion {
            caps,
            size_bits,
            _shared_status: PhantomData,
        })
    }

    pub(super) fn as_strong<SizeBits: Unsigned>(
        self,
    ) -> Result<UnmappedMemoryRegion<SizeBits, SS, CapRole, MemKind>, VSpaceError>
    where
        // Forces regions to be page-aligned.
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        if self.size_bits != SizeBits::U8 {
            return Err(VSpaceError::InvalidRegionSize);
        }
        Ok(UnmappedMemoryRegion::from_caps(CapRange::new(
            self.caps.start_cptr,
            self.caps.start_cap_data,
        )))
    }

    pub fn to_shared(self) -> WeakUnmappedMemoryRegion<shared_status::Shared, CapRole, MemKind> {
        WeakUnmappedMemoryRegion {
            caps: self.caps,
            size_bits: self.size_bits,
            _shared_status: PhantomData,
        }
    }
}

pub struct WeakMappedMemoryRegion<SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind> {
    pub(super) caps: WeakCapRange<Page<page_state::Mapped, MemKind>, CapRole>,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl<SS: SharedStatus, CapRole: CNodeRole, MemKind: MemoryKind>
    WeakMappedMemoryRegion<SS, CapRole, MemKind>
{
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

    pub(super) fn try_from_caps(
        caps: WeakCapRange<Page<page_state::Mapped, MemKind>, CapRole>,
        size_bits: u8,
    ) -> Result<WeakMappedMemoryRegion<SS, CapRole, MemKind>, InvalidSizeBits> {
        if num_pages(size_bits)? != caps.len() {
            return Err(InvalidSizeBits::SizeBitsMismatchPageCapCount);
        }
        Ok(WeakMappedMemoryRegion {
            caps,
            size_bits,
            _shared_status: PhantomData,
        })
    }

    pub(super) fn as_strong<SizeBits: Unsigned>(
        self,
    ) -> Result<MappedMemoryRegion<SizeBits, SS, CapRole, MemKind>, VSpaceError>
    where
        // Forces regions to be page-aligned.
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        if self.size_bits != SizeBits::U8 {
            return Err(VSpaceError::InvalidRegionSize);
        }
        Ok(MappedMemoryRegion::from_caps(CapRange::new(
            self.caps.start_cptr,
            self.caps.start_cap_data,
        )))
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum InvalidSizeBits {
    TooSmallToRepresentAPage,
    SizeBitsMismatchPageCapCount,
    SizeBitsTooBig,
}

pub(super) fn num_pages(size_bits: u8) -> Result<usize, InvalidSizeBits> {
    if size_bits < PageBits::U8 {
        return Err(InvalidSizeBits::TooSmallToRepresentAPage);
    }
    Ok(2usize
        .checked_pow(u32::from(size_bits) - PageBits::U32)
        .ok_or_else(|| InvalidSizeBits::SizeBitsTooBig)?)
}
