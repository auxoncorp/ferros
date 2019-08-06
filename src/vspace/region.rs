use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use super::VSpaceError;
use crate::arch::{self, PageBits};
use crate::cap::{
    granule_state, memory_kind, role, CNode, CNodeRole, CNodeSlots, CapRange, Granule,
    GranuleSlotCount, GranuleState, InternalASID, LocalCNodeSlots, LocalCap, MemoryKind,
    RetypeError, Untyped, WCNodeSlots, WUntyped, WeakCapRange, WeakMemoryKind,
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
#[allow(type_alias_bounds)]
pub type UnmappedMemoryRegion<SizeBits, ShStatus, CapRole: CNodeRole = role::Local> =
    MemoryRegion<granule_state::Unmapped, SizeBits, ShStatus, CapRole>;
/// A memory region which is mapped into an address space, meaning it
/// has a virtual address and an associated asid in which that virtual
/// address is valid.
#[allow(type_alias_bounds)]
pub type MappedMemoryRegion<SizeBits, ShStatus, CapRole: CNodeRole = role::Local> =
    MemoryRegion<granule_state::Mapped, SizeBits, ShStatus, CapRole>;
#[allow(type_alias_bounds)]
pub type WeakUnmappedMemoryRegion<ShStatus, CapRole: CNodeRole = role::Local> =
    WeakMemoryRegion<granule_state::Unmapped, ShStatus, CapRole>;
#[allow(type_alias_bounds)]
pub type WeakMappedMemoryRegion<ShStatus, CapRole: CNodeRole = role::Local> =
    WeakMemoryRegion<granule_state::Mapped, ShStatus, CapRole>;

/// A `1 << SizeBits` bytes region of memory. It can be
/// shared or owned exclusively. The ramifications of its shared
/// status are described more completely in the `mapped_shared_region`
/// function description.
pub struct MemoryRegion<
    State: GranuleState,
    SizeBits: Unsigned,
    SS: SharedStatus,
    CapRole: CNodeRole = role::Local,
> {
    pub(super) caps: WeakCapRange<Granule<State>, CapRole>,
    pub(super) kind: WeakMemoryKind,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<State: GranuleState, SizeBits: Unsigned, CapRole: CNodeRole>
    MemoryRegion<State, SizeBits, shared_status::Exclusive, CapRole>
{
    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// memory region, marked as shared.
    pub fn share<CNodeSlotCount: Unsigned, DestRole: CNodeRole>(
        self,
        slots: CNodeSlots<CNodeSlotCount, DestRole>,
        cnode: &LocalCap<CNode<CapRole>>,
        rights: CapRights,
    ) -> Result<
        (
            MemoryRegion<granule_state::Unmapped, SizeBits, shared_status::Shared, DestRole>,
            MemoryRegion<State, SizeBits, shared_status::Shared, CapRole>,
        ),
        VSpaceError,
    > {
        let new_region = self.share_private(slots, cnode, rights)?;
        let first_granule = self.caps.iter().first().unwrap();
        Ok((
            new_region,
            // The one that came in which may or may not be mapped but
            // is definitely shared.
            MemoryRegion {
                caps: WeakCapRange::new(
                    first_granule.cptr,
                    Granule {
                        size: first_granule.size,
                        state: first_granule.state,
                    },
                    self.caps.count(),
                ),
                kind: self.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
        ))
    }
}

impl<State: GranuleState, SizeBits: Unsigned, CapRole: CNodeRole>
    MemoryRegion<State, SizeBits, shared_status::Shared, CapRole>
{
    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// memory region, marked as shared.
    pub fn share<CNodeSlotCount: Unsigned, DestRole: CNodeRole>(
        &self,
        slots: CNodeSlots<CNodeSlotCount, DestRole>,
        cnode: &LocalCap<CNode<CapRole>>,
        rights: CapRights,
    ) -> Result<
        (
            MemoryRegion<granule_state::Unmapped, SizeBits, shared_status::Shared, DestRole>,
            MemoryRegion<State, SizeBits, shared_status::Shared, CapRole>,
        ),
        VSpaceError,
    > {
        self.share_private(slots, cnode, rights)
    }
}

impl<State: GranuleState, SizeBits: Unsigned, SS: SharedStatus, CapRole: CNodeRole>
    MemoryRegion<State, SizeBits, SS, CapRole>
{
    pub const SIZE_BYTES: usize = 1 << SizeBits::USIZE;

    /// The number of bits needed to address this region
    pub fn size_bits(&self) -> u8 {
        SizeBits::U8
    }

    /// The size of this region in bytes.
    pub fn size_bytes(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub(super) fn from_caps(
        caps: WeakCapRange<Granule<State>, CapRole>,
        kind: WeakMemoryKind,
    ) -> MemoryRegion<State, SizeBits, SS, CapRole> {
        MemoryRegion {
            caps,
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub fn weaken(self) -> WeakMemoryRegion<State, SS, CapRole> {
        WeakMemoryRegion::try_from_caps(self.caps.weaken(), self.kind, SizeBits::U8)
            .expect("Cap page slots to memory region size invariant maintained by type signature")
    }

    fn share_private<CNodeSlotCount: Unsigned, DestRole: CNodeRole>(
        &self,
        slots: CNodeSlots<CNodeSlotCount, DestRole>,
        cnode: &LocalCap<CNode<CapRole>>,
        rights: CapRights,
    ) -> Result<UnmappedMemoryRegion<SizeBits, shared_status::Shared, DestRole>, VSpaceError> {
        if self.caps.len > CNodeSlotCount::USIZE {
            return Err(VSpaceError::InsufficientCNodeSlots);
        }

        let pages_offset = self.caps.start_cptr;
        let original_mapped_state = self.caps.start_cap_data.state.clone();
        let slots_offset = slots.cap_data.offset;
        for (slot, page) in slots.iter().zip(self.caps.into_iter()) {
            let _ = page.copy(cnode, slot, rights)?;
        }

        let granule_info = arch::determine_best_granule_fit(Self::SizeBits::U8);

        Ok(MemoryRegion {
            caps: WeakCapRange::new(
                slots_offset,
                Granule {
                    size: granule_info.size_bits,
                    state: granule_state::Unmapped,
                },
                granule_info.count,
            ),
            kind: self.kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        })
    }
}

impl<SizeBits: Unsigned> UnmappedMemoryRegion<SizeBits, shared_status::Exclusive> {
    /// construct a strongly typed (size indexed) unmapped memory
    /// region.
    pub fn new(
        slots: LocalCNodeSlots<GranuleSlotCount<SizeBits>>,
        ut: LocalCap<Untyped<SizeBits>>,
    ) -> Result<Self, VSpaceError>
    where
        // Ensure that a region is at least one page in size.
        SizeBits: IsGreaterOrEqual<PageBits>,
    {
        let grans = ut.weaken().retype_memory(&mut slots.weaken())?;
        MemoryRegion {
            caps: grans,
            kind: memory_kind::General {},
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion::from_caps(self.caps, self.kind)
    }
}

impl<SizeBits: Unsigned, SS: SharedStatus> MappedMemoryRegion<SizeBits, SS> {
    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion {
            caps: WeakCapRange::new(
                self.caps.start_cptr,
                Granule {
                    size_bits: self.size_bits,
                    type_id: self.type_id,
                    state: granule_state::Mapped {
                        vaddr: self.vaddr(),
                        asid: self.asid(),
                    },
                },
            ),
            kind: self.kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }
}

pub struct WeakMemoryRegion<State: GranuleState, SS: SharedStatus, CapRole: CNodeRole = role::Local>
{
    pub(super) caps: WeakCapRange<Granule<State>, CapRole>,
    pub(super) kind: WeakMemoryKind,
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
        Ok(WeakMemoryRegion {
            caps,
            kind,
            size_bits,
            _shared_status: PhantomData,
        })
    }
}
impl<State: GranuleState, SS: SharedStatus> WeakMemoryRegion<State, SS, role::Local> {
    pub(super) fn unchecked_new(
        local_page_caps_offset_cptr: usize,
        state: State,
        kind: WeakMemoryKind,
        size_bits: u8,
    ) -> Self {
        let gran_info = arch::determine_best_granule_fit(size_bits);
        WeakMemoryRegion {
            caps: WeakCapRange::new(
                local_page_caps_offset_cptr,
                Granule {
                    size_bits: gran_info.size_bits,
                    type_id: gran_info.type_id,
                    state,
                },
                gran_info.count,
            ),
            kind,
            size_bits,
            _shared_status: PhantomData,
        }
    }
}
impl<State: GranuleState, SS: SharedStatus, CapRole: CNodeRole>
    WeakMemoryRegion<State, SS, CapRole>
{
    /// The number of bits needed to address this region
    pub fn size_bits(&self) -> u8 {
        self.size_bits
    }

    /// The size of this region in bytes.
    pub fn size_bytes(&self) -> usize {
        2usize.pow(u32::from(self.size_bits))
    }
    pub(super) fn try_from_caps(
        caps: WeakCapRange<Granule<State>, CapRole>,
        kind: WeakMemoryKind,
        size_bits: u8,
    ) -> Result<WeakMemoryRegion<State, SS, CapRole>, InvalidSizeBits> {
        let gran_info = arch::determine_best_granule_fit(size_bits);
        if gran_info.count != caps.len() {
            return Err(InvalidSizeBits::SizeBitsMismatchCapCount);
        }
        Ok(WeakMemoryRegion {
            caps,
            kind,
            size_bits,
            _shared_status: PhantomData,
        })
    }

    pub(super) fn as_strong<SizeBits: Unsigned>(
        self,
    ) -> Result<MemoryRegion<State, SizeBits, SS, CapRole>, VSpaceError>
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
        Ok(MemoryRegion::from_caps(
            CapRange::new(self.caps.start_cptr, self.caps.start_cap_data),
            self.kind,
        ))
    }

    pub fn to_shared(self) -> WeakMemoryRegion<State, shared_status::Shared, CapRole> {
        WeakMemoryRegion {
            caps: self.caps,
            kind: self.kind,
            size_bits: self.size_bits,
            _shared_status: PhantomData,
        }
    }
}

impl<SS: SharedStatus, CapRole: CNodeRole> WeakMappedMemoryRegion<SS, CapRole> {
    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum InvalidSizeBits {
    SizeBitsMismatchCapCount,
}
