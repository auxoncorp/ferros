use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use crate::arch::{self, GranuleSlotsWorstCase, PageBits, G1, G2, G3, G4};
use crate::cap::{
    granule_state, memory_kind, role, CNode, CNodeRole, CNodeSlots, Cap, Granule, GranuleCondExpr,
    GranuleCountExpr, GranuleSlotCount, GranuleState, GranuleSubCond1, GranuleSubCond2, IToUUnsafe,
    InternalASID, LocalCNodeSlots, LocalCap, MemoryKind, Page, RetypeError, Untyped, WCNodeSlots,
    WUntyped, WeakCapRange, WeakMemoryKind,
};

use crate::_IfThenElse;
use crate::pow::_Pow;
use crate::userland::CapRights;

use super::VSpaceError;

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
> where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
{
    pub(super) caps: WeakCapRange<Granule<State>, CapRole>,
    pub(super) kind: WeakMemoryKind,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<State: GranuleState, SizeBits: Unsigned, SS: SharedStatus, CapRole: CNodeRole>
    MemoryRegion<State, SizeBits, SS, CapRole>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
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

    pub(super) fn unchecked_new(
        local_page_caps_offset_cptr: usize,
        state: State,
        kind: WeakMemoryKind,
    ) -> Self {
        let gran_info = arch::determine_best_granule_fit(SizeBits::U8);

        MemoryRegion {
            caps: WeakCapRange::new(
                local_page_caps_offset_cptr,
                Granule {
                    size_bits: gran_info.size_bits,
                    type_id: gran_info.type_id,
                    state: state,
                },
                gran_info.count as usize,
            ),
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    /// Convert this region into a runtime-tracked region.
    pub fn weaken(self) -> WeakMemoryRegion<State, SS, CapRole> {
        WeakMemoryRegion::try_from_caps(self.caps, self.kind, SizeBits::U8)
            .expect("Cap page slots to memory region size invariant maintained by type signature")
    }

    /// N.B. until MemoryKind tracking is added to Page, this is a lossy conversion
    /// that will assume the Region was for General memory
    pub(crate) fn to_page(self) -> LocalCap<Page<State>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: Page {
                state: self.caps.start_cap_data.state,
            },
            _role: PhantomData,
        }
    }

    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// memory region, marked as shared.
    pub fn share<DestRole: CNodeRole>(
        self,
        slots: CNodeSlots<GranuleSlotsWorstCase, DestRole>,
        cnode: &LocalCap<CNode<CapRole>>,
        rights: CapRights,
    ) -> Result<
        (
            MemoryRegion<granule_state::Unmapped, SizeBits, shared_status::Shared, DestRole>,
            MemoryRegion<State, SizeBits, shared_status::Shared, CapRole>,
        ),
        VSpaceError,
    > {
        let offset = self.caps.start_cptr;
        let range_length = self.caps.len;
        let original_mapped_state = self.caps.start_cap_data.state.clone();
        let dest_offset = slots.cap_data.offset;
        for (slot, gran) in slots.iter().zip(self.caps.into_iter()) {
            let _ = gran.copy(cnode, slot, rights)?;
        }

        Ok((
            MemoryRegion::unchecked_new(offset, granule_state::Unmapped, self.kind),
            MemoryRegion::from_caps(
                WeakCapRange::new(
                    dest_offset,
                    Granule {
                        state: original_mapped_state,
                        type_id: self.caps.start_cap_data.type_id,
                        size_bits: self.caps.start_cap_data.size_bits,
                    },
                    range_length,
                ),
                self.kind,
            ),
        ))
    }
}

impl LocalCap<Page<granule_state::Unmapped>> {
    /// N.B. until MemoryKind tracking is added to Page, this is a lossy conversion
    /// that will assume the Page was for General memory
    pub(crate) fn to_region(
        self,
    ) -> MemoryRegion<granule_state::Unmapped, PageBits, shared_status::Exclusive> {
        MemoryRegion::unchecked_new(self.cptr, self.cap_data.state, WeakMemoryKind::General)
    }
}

impl<SizeBits: Unsigned> UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
{
    /// Retype the necessary number of granules into memory
    /// capabilities and return the unmapped region.
    pub fn new(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: LocalCNodeSlots<GranuleSlotCount<SizeBits>>,
    ) -> Result<Self, VSpaceError>
    where
        SizeBits: NonZero,
        SizeBits: IsGreaterOrEqual<G1>,
        SizeBits: IsGreaterOrEqual<G2>,
        SizeBits: IsGreaterOrEqual<G3>,
        SizeBits: IsGreaterOrEqual<G4, Output = True>,

        PInt<SizeBits>: Sub<PInt<G1>>,
        <PInt<SizeBits> as Sub<PInt<G1>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G2>>,
        <PInt<SizeBits> as Sub<PInt<G2>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G3>>,
        <PInt<SizeBits> as Sub<PInt<G3>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G4>>,
        <PInt<SizeBits> as Sub<PInt<G4>>>::Output: IToUUnsafe,

        <SizeBits as IsGreaterOrEqual<G1>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G1>>>::Output as IToUUnsafe>::Uint,
            GranuleSubCond2<SizeBits>,
        >,

        <SizeBits as IsGreaterOrEqual<G2>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G2>>>::Output as IToUUnsafe>::Uint,
            GranuleSubCond1<SizeBits>,
        >,

        <SizeBits as IsGreaterOrEqual<G3>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G3>>>::Output as IToUUnsafe>::Uint,
            <<PInt<SizeBits> as Sub<PInt<G4>>>::Output as IToUUnsafe>::Uint,
        >,

        GranuleCondExpr<SizeBits>: _Pow,

        GranuleCountExpr<SizeBits>: Unsigned,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.weaken().retype_memory(&mut slots.weaken())?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    pub fn new_device(
        ut: LocalCap<Untyped<SizeBits, memory_kind::Device>>,
        slots: LocalCNodeSlots<GranuleSlotCount<SizeBits>>,
    ) -> Result<Self, VSpaceError>
    where
        SizeBits: NonZero,
        SizeBits: IsGreaterOrEqual<G1>,
        SizeBits: IsGreaterOrEqual<G2>,
        SizeBits: IsGreaterOrEqual<G3>,
        SizeBits: IsGreaterOrEqual<G4, Output = True>,

        PInt<SizeBits>: Sub<PInt<G1>>,
        <PInt<SizeBits> as Sub<PInt<G1>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G2>>,
        <PInt<SizeBits> as Sub<PInt<G2>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G3>>,
        <PInt<SizeBits> as Sub<PInt<G3>>>::Output: IToUUnsafe,

        PInt<SizeBits>: Sub<PInt<G4>>,
        <PInt<SizeBits> as Sub<PInt<G4>>>::Output: IToUUnsafe,

        <SizeBits as IsGreaterOrEqual<G1>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G1>>>::Output as IToUUnsafe>::Uint,
            GranuleSubCond2<SizeBits>,
        >,

        <SizeBits as IsGreaterOrEqual<G2>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G2>>>::Output as IToUUnsafe>::Uint,
            GranuleSubCond1<SizeBits>,
        >,

        <SizeBits as IsGreaterOrEqual<G3>>::Output: _IfThenElse<
            <<PInt<SizeBits> as Sub<PInt<G3>>>::Output as IToUUnsafe>::Uint,
            <<PInt<SizeBits> as Sub<PInt<G4>>>::Output as IToUUnsafe>::Uint,
        >,

        GranuleCondExpr<SizeBits>: _Pow,

        GranuleCountExpr<SizeBits>: Unsigned,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.weaken().retype_memory(&mut slots.weaken())?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion::from_caps(self.caps, self.kind)
    }
}

impl<SizeBits: Unsigned, SS: SharedStatus> MappedMemoryRegion<SizeBits, SS>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
{
    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion::unchecked_new(
            self.caps.start_cptr,
            granule_state::Mapped {
                vaddr: self.vaddr(),
                asid: self.asid(),
            },
            self.kind,
        )
    }
}

pub struct WeakMemoryRegion<State: GranuleState, SS: SharedStatus, CapRole: CNodeRole = role::Local>
{
    pub(super) caps: WeakCapRange<Granule<State>, CapRole>,
    pub(super) kind: WeakMemoryKind,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl WeakMemoryRegion<granule_state::Unmapped, shared_status::Exclusive> {
    pub fn new<MemKind: MemoryKind>(
        untyped: LocalCap<WUntyped<MemKind>>,
        slots: &mut WCNodeSlots,
    ) -> Result<Self, RetypeError> {
        let kind = untyped.cap_data.kind.weaken();
        let size_bits = untyped.size_bits();
        let caps = untyped.retype_memory(slots)?;
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
                    type_id: gran_info.type_id,
                    size_bits: gran_info.size_bits,
                    state: state,
                },
                gran_info.count as usize,
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

        if gran_info.count as usize != caps.len() {
            return Err(InvalidSizeBits::SizeBitsMismatchPageCapCount);
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
    {
        if self.size_bits != SizeBits::U8 {
            return Err(VSpaceError::InvalidRegionSize);
        }
        Ok(MemoryRegion::from_caps(self.caps, self.kind))
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
