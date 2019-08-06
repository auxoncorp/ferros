//! A VSpace represents the virtual address space of a process in
//! seL4.
//!
//! This architecture-independent realization of that concept uses
//! memory _regions_ rather than expose the granules that each layer
//! in the addressing structures is responsible for mapping.
use core::marker::PhantomData;
use core::ops::Sub;
use core::ptr;

use typenum::*;

use crate::alloc::ut_buddy::{self, UTBuddyError, WUTBuddy};
use crate::arch::cap::{AssignedASID, UnassignedASID};
use crate::arch::{self, AddressSpace, PageBits, PageBytes, PagingRoot};
use crate::bootstrap::UserImage;
use crate::cap::{
    granule_state, memory_kind, role, CNodeRole, CNodeSlots, Cap, CapType, ChildCNodeSlot,
    DirectRetype, Granule, GranuleSlotCount, InternalASID, LocalCNode, LocalCNodeSlots, LocalCap,
    Page, PhantomCap, RetypeError, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped, WeakCapRange,
    WeakCopyError,
};
use crate::error::SeL4Error;
use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;
mod region;
pub use region::*;

include!(concat!(env!("OUT_DIR"), "/KERNEL_RETYPE_FAN_OUT_LIMIT"));

pub trait VSpaceState: private::SealedVSpaceState {}

pub mod vspace_state {
    use super::VSpaceState;

    /// A VSpace state where there is a blank address space and the
    /// capability to do some mapping, but no awareness of the
    /// user image or mappings. The root vspace should never be in this
    /// state in user-land code.
    pub struct Empty;
    impl VSpaceState for Empty {}

    /// A VSpace state where the address space takes into account
    /// the presence of the user image and reserved regions of
    /// the address space. All unclaimed address space is fair game
    /// for the VSpace to use.
    pub struct Imaged;
    impl VSpaceState for Imaged {}
}

/// A `Maps` implementor is a paging layer that maps granules of type
/// `LowerLevel`. If this layer isn't present for the incoming address,
/// `MappingError::Overflow` should be returned, as this signals to
/// the caller—the layer above—that it needs to create a new object at
/// this layer and then attempt again to map the `item`.
///
/// N.B. A "Granule" is "one of the constituent members of a layer", or
/// "the level one level down from the current level".
pub trait Maps<LowerLevel: CapType> {
    /// Map the level/layer down relative to this layer.
    /// E.G. for a PageTable, this would map a Page.
    /// E.G. for a PageDirectory, this would map a PageTable.
    fn map_granule<RootLowerLevel, Root>(
        &mut self,
        item: &LocalCap<LowerLevel>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootLowerLevel>,
        Root: CapType,
        RootLowerLevel: CapType;
}

#[derive(Debug)]
/// The error type returned when there is in an error in the
/// construction of any of the intermediate layers of the paging
/// structure.
pub enum MappingError {
    /// Overflow is the special variant that signals to the caller
    /// that this layer is missing and the intermediate-layer mapping
    /// ought to roll up an additional layer.
    Overflow,
    AddrNotPageAligned,
    /// In all seL4-support architectures, a page is the smallest
    /// granule; it aligns with a physical frame of memory. This error
    /// is broken out to differentiate between a failure at the leaf
    /// rather than during branch construction.
    PageMapFailure(SeL4Error),
    /// A failure to map one of the intermediate layers.
    IntermediateLayerFailure(SeL4Error),
    /// The error was specific the allocation of an untyped preceeding
    /// a `seL4_Untyped_Retype` call to create a capability for an
    /// intermediate layer.
    UTBuddyError(UTBuddyError),
    /// The error was specific to retyping the untyped memory the
    /// layers thread through during their mapping. This likely
    /// signals that this VSpace is out of resources with which to
    /// convert to intermediate structures.
    RetypeError(RetypeError),
}

impl From<UTBuddyError> for MappingError {
    fn from(e: UTBuddyError) -> Self {
        MappingError::UTBuddyError(e)
    }
}

impl From<RetypeError> for MappingError {
    fn from(e: RetypeError) -> Self {
        MappingError::RetypeError(e)
    }
}

#[derive(Debug)]
/// The error type returned by VSpace operations.
pub enum VSpaceError {
    /// An error occurred when mapping a region.
    MappingError(MappingError),
    /// An error occurred when retyping a region to an
    /// `UnmappedMemoryRegion`.
    RetypeRegion(RetypeError),
    /// A wrapper around the top-level syscall error type.
    SeL4Error(SeL4Error),
    /// There are no more slots in which to place retyped layer caps.
    InsufficientCNodeSlots,
    /// An attempted mapping would have overflowed the maximum addressable range (core::usize::MAX)
    ExceededAddressableSpace,
    /// Internal watermarking has determined that the desired region mapping would
    /// not fit in available unclaimed address space.
    InsufficientAddressSpaceAvailableToMapRegion,
    ASIDMismatch,

    /// This error is returned by `map_region_at_addr` its rollback
    /// collection is not large enough to hold the number of pages, it's
    /// arbitrary and we'll need to address this when we get to doing
    /// special-sized granules.
    TriedToMapTooManyPagesAtOnce,
    InvalidRegionSize,
}

impl From<RetypeError> for VSpaceError {
    fn from(e: RetypeError) -> VSpaceError {
        VSpaceError::RetypeRegion(e)
    }
}

impl From<SeL4Error> for VSpaceError {
    fn from(e: SeL4Error) -> VSpaceError {
        VSpaceError::SeL4Error(e)
    }
}

/// A `PagingLayer` is a mapping-layer in an architecture's address
/// space structure.
pub trait PagingLayer {
    /// The `Item` is the granule which this layer maps.
    type Item: CapType + DirectRetype + PhantomCap;

    /// A function which attempts to map this layer's granule at the
    /// given address. If the error is a seL4 lookup error, then the
    /// implementor ought to return `MappingError::Overflow` to signal
    /// that mapping is needed at the layer above, otherwise the error
    /// is just bubbled up to the caller.
    fn map_layer<RootLowerLevel: CapType, Root>(
        &mut self,
        item: &LocalCap<Self::Item>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        utb: &mut WUTBuddy,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootLowerLevel>,
        Root: CapType;
}

/// `PagingTop` represents the root of an address space structure.
pub struct PagingTop<LowerLevel, CurrentLevel: Maps<LowerLevel>>
where
    CurrentLevel: CapType,
    LowerLevel: CapType,
{
    pub layer: CurrentLevel,
    pub(super) _item: PhantomData<LowerLevel>,
}

impl<LowerLevel, CurrentLevel: Maps<LowerLevel>> PagingLayer for PagingTop<LowerLevel, CurrentLevel>
where
    CurrentLevel: CapType,
    LowerLevel: CapType + DirectRetype + PhantomCap,
{
    type Item = LowerLevel;
    fn map_layer<RootLowerLevel: CapType, Root>(
        &mut self,
        item: &LocalCap<LowerLevel>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        _utb: &mut WUTBuddy,
        _slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootLowerLevel>,
        Root: CapType,
    {
        self.layer
            .map_granule(item, addr, root, rights, vm_attributes)
    }
}

/// `PagingRec` represents an intermediate layer. It is of type `CurrentLevel`,
/// while it maps `LowerLevel`s. The layer above it is `UpperLevel`.
pub struct PagingRec<LowerLevel: CapType, CurrentLevel: Maps<LowerLevel>, UpperLevel: PagingLayer> {
    pub(crate) layer: CurrentLevel,
    pub(crate) next: UpperLevel,
    pub(crate) _item: PhantomData<LowerLevel>,
}

impl<LowerLevel, CurrentLevel: Maps<LowerLevel>, UpperLevel: PagingLayer> PagingLayer
    for PagingRec<LowerLevel, CurrentLevel, UpperLevel>
where
    CurrentLevel: CapType,
    LowerLevel: CapType + DirectRetype + PhantomCap,
{
    type Item = LowerLevel;
    fn map_layer<RootLowerLevel: CapType, Root>(
        &mut self,
        item: &LocalCap<LowerLevel>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        utb: &mut WUTBuddy,
        mut slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootLowerLevel>,
        Root: CapType,
    {
        // Attempt to map this layer's granule.
        match self
            .layer
            .map_granule(item, addr, root, rights, vm_attributes)
        {
            // if it fails with a lookup error, ask the next layer up
            // to map a new instance at this layer.
            Err(MappingError::Overflow) => {
                let ut = utb.alloc(slots, <UpperLevel::Item as DirectRetype>::SizeBits::U8)?;
                let next_item = ut.retype::<UpperLevel::Item>(&mut slots)?;
                self.next
                    .map_layer(&next_item, addr, root, rights, vm_attributes, utb, slots)?;
                // Then try again to map this layer.
                self.layer
                    .map_granule(item, addr, root, rights, vm_attributes)
            }
            // Any other result (success \/ other failure cases) can
            // be returned as is.
            res => res,
        }
    }
}

pub enum ProcessCodeImageConfig<'a> {
    ReadOnly,
    /// Use when you need to be able to write to statics in the child process
    ReadWritable {
        // We really just need the vadder
        parent_mapped_region: &'a mut MappedMemoryRegion<
            crate::arch::TotalCodeSizeBits,
            shared_status::Shared,
            role::Local,
        >,
        code_pages_ut: LocalCap<Untyped<crate::arch::TotalCodeSizeBits>>,
    },
}

/// A virtual address space manager.
///
/// CapRole indicates whether the capabilities related to manipulating this VSpace
/// are accessible from the current thread's CSpace, or from a child's CSpace
pub struct VSpace<State: VSpaceState = vspace_state::Imaged, CapRole: CNodeRole = role::Local> {
    /// The cap to this address space's root-of-the-tree item.
    root: Cap<PagingRoot, CapRole>,
    /// The id of this address space.
    asid: InternalASID,
    /// The recursive structure which represents an address space
    /// structure. `AddressSpace` is a type which is exported by
    /// `crate::arch` and has architecture specific implementations.
    layers: AddressSpace,
    /// The following two members are the resources used by the VSpace
    /// when building out intermediate layers.
    untyped: WUTBuddy<CapRole>,
    slots: Cap<WCNodeSlotsData<CapRole>, CapRole>,
    available_address_range: AvailableAddressRange,
    _state: PhantomData<State>,
}

impl VSpace<vspace_state::Empty, role::Local> {
    pub(crate) fn new(
        mut root_cap: LocalCap<PagingRoot>,
        asid: LocalCap<UnassignedASID>,
        slots: WCNodeSlots,
        untyped: LocalCap<WUntyped<memory_kind::General>>,
    ) -> Result<Self, VSpaceError> {
        let assigned_asid = asid.assign(&mut root_cap)?;
        Ok(VSpace {
            root: root_cap,
            asid: assigned_asid.cap_data.asid,
            layers: AddressSpace::new(),
            untyped: ut_buddy::weak_ut_buddy(untyped),
            slots,
            available_address_range: AvailableAddressRange::default(),
            _state: PhantomData,
        })
    }
}

impl<State: VSpaceState, CapRole: CNodeRole> VSpace<State, CapRole> {
    /// This address space's id.
    pub(crate) fn asid(&self) -> InternalASID {
        self.asid
    }

    pub(crate) fn root_cptr(&self) -> usize {
        self.root.cptr
    }
}

impl<State: VSpaceState> VSpace<State, role::Local> {
    /// A thin wrapper around self.layers.map_layer that reduces the amount
    /// of repetitive, visible self-reference
    fn map_page_at_addr_without_watermarking(
        &mut self,
        page: LocalCap<Granule<granule_state::Unmapped>>,
        address: usize,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<LocalCap<Granule<granule_state::Mapped>>, VSpaceError> {
        self.layers
            .map_layer(
                &page,
                address,
                &mut self.root,
                rights,
                vm_attributes,
                &mut self.untyped,
                &mut self.slots,
            )
            .map(|_| Cap {
                cptr: page.cptr,
                _role: PhantomData,
                cap_data: Granule {
                    state: granule_state::Mapped {
                        asid: self.asid,
                        vaddr: address,
                    },
                },
            })
            .map_err(|e| match e {
                MappingError::PageMapFailure(se) | MappingError::IntermediateLayerFailure(se) => {
                    VSpaceError::SeL4Error(se)
                }
                e => VSpaceError::MappingError(e),
            })
    }
}

impl VSpace<vspace_state::Imaged, role::Local> {
    /// Unmap a region.
    pub fn unmap_region<SizeBits: Unsigned, SS: SharedStatus>(
        &mut self,
        region: MappedMemoryRegion<SizeBits, SS>,
    ) -> Result<UnmappedMemoryRegion<SizeBits, SS>, VSpaceError> {
        self.weak_unmap_region(region.weaken())
            .and_then(|r| r.as_strong::<SizeBits>())
    }
    /// Unmap a weak region.
    pub fn weak_unmap_region<SS: SharedStatus>(
        &mut self,
        region: WeakMappedMemoryRegion<SS>,
    ) -> Result<WeakUnmappedMemoryRegion<SS>, VSpaceError> {
        if self.asid != region.asid() {
            return Err(VSpaceError::ASIDMismatch);
        }
        let start_cptr = region.caps.start_cptr;
        let slot_count = region.caps.len;
        let size_bits = region.size_bits();
        for page_cap in region.caps.into_iter() {
            let _ = self.unmap_page(page_cap)?;
        }
        Ok(WeakMemoryRegion::unchecked_new(
            start_cptr,
            slot_count,
            granule_state::Unmapped,
            region.kind,
            size_bits,
        ))
    }

    fn unmap_page(
        &mut self,
        page: LocalCap<Granule<granule_state::Mapped>>,
    ) -> Result<LocalCap<Granule<granule_state::Unmapped>>, SeL4Error> {
        page.unmap()
    }

    // This function will move the caps into the child's CSpace so
    // that it may use it.
    pub(crate) fn for_child(
        self,
        src_cnode: &LocalCap<LocalCNode>,
        child_root_slot: ChildCNodeSlot,
        mut ut_transfer_slots: LocalCap<WCNodeSlotsData<role::Child>>,
        child_paging_slots: Cap<WCNodeSlotsData<role::Child>, role::Child>,
    ) -> Result<VSpace<vspace_state::Imaged, role::Child>, VSpaceError> {
        let VSpace {
            root,
            asid,
            layers,
            untyped,
            slots: _,
            available_address_range,
            ..
        } = self;
        let child_root = root.move_to_slot(src_cnode, child_root_slot)?;
        let child_untyped = untyped
            .move_to_child(src_cnode, &mut ut_transfer_slots)
            .map_err(|e| match e {
                UTBuddyError::NotEnoughSlots => VSpaceError::InsufficientCNodeSlots,
                UTBuddyError::SeL4Error(se) => VSpaceError::SeL4Error(se),
                _ => unreachable!(
                    "All other UTBuddyError variants are irrelevant for the move_to_child call"
                ),
            })?;
        Ok(VSpace {
            root: child_root,
            asid,
            layers,
            untyped: child_untyped,
            slots: child_paging_slots,
            available_address_range,
            _state: PhantomData,
        })
    }

    pub fn new(
        paging_root: LocalCap<PagingRoot>,
        asid: LocalCap<UnassignedASID>,
        mut slots: WCNodeSlots,
        paging_untyped: LocalCap<WUntyped<memory_kind::General>>,
        // Things relating to user image code
        code_image_config: ProcessCodeImageConfig,
        user_image: &UserImage<role::Local>,
        parent_cnode: &LocalCap<LocalCNode>,
    ) -> Result<Self, VSpaceError> {
        let code_slots = match slots.alloc(user_image.pages_count()) {
            Ok(t) => t,
            Err(_) => return Err(VSpaceError::InsufficientCNodeSlots),
        };
        let mut vspace =
            VSpace::<vspace_state::Empty>::new(paging_root, asid, slots, paging_untyped)?;

        // Map the code image into the process VSpace
        match code_image_config {
            ProcessCodeImageConfig::ReadOnly => {
                for (ui_granule, slot) in
                    user_image.granule_iter().zip(code_slots.into_strong_iter())
                {
                    let address = ui_granule.cap_data.state.vaddr;
                    let copied_page_cap = ui_granule.copy(&parent_cnode, slot, CapRights::R)?;
                    let _ = vspace.map_page_at_addr_without_watermarking(
                        copied_page_cap,
                        address,
                        CapRights::R,
                        arch::vm_attributes::DEFAULT,
                    )?;
                    vspace
                        .available_address_range
                        .observe_mapping(address, ui_granule.size_bits)
                }
            }
            ProcessCodeImageConfig::ReadWritable {
                parent_shared_region,
                code_slots,
            } => {
                ptr::copy_nonoverlapping(
                    // W/r/t the unwrap: If there were no granules
                    // in the user image we wouldn't be here right
                    // now.
                    user_image.granules_iter().first().unwrap().vaddr() as *const u8,
                    parent_shared_region.vaddr() as *mut u8,
                    user_image.size_bytes(),
                );
                let child_region = parent_shared_region.share(code_slots)?;
                vspace.map_region_at_addr(
                    child_region,
                    user_image.vaddr(),
                    CapRights::RW,
                    arch::vm_attributes::DEFAULT,
                )?;
            }
        }

        Ok(VSpace {
            root: vspace.root,
            asid: vspace.asid,
            layers: vspace.layers,
            untyped: vspace.untyped,
            slots: vspace.slots,
            available_address_range: vspace.available_address_range,
            _state: PhantomData,
        })
    }

    /// `bootstrap` is used to wrap the root thread's address space.
    pub(crate) fn bootstrap(
        root_vspace_cptr: usize,
        next_addr: usize,
        cslots: WCNodeSlots,
        asid: LocalCap<AssignedASID>,
        ut: LocalCap<WUntyped<memory_kind::General>>,
    ) -> Self {
        let mut available_address_range = AvailableAddressRange::default();
        available_address_range.bottom = next_addr;
        VSpace {
            layers: AddressSpace::new(),
            root: Cap {
                cptr: root_vspace_cptr,
                cap_data: PagingRoot::phantom_instance(),
                _role: PhantomData,
            },
            untyped: ut_buddy::weak_ut_buddy(ut),
            slots: cslots,
            available_address_range,
            asid: asid.cap_data.asid,
            _state: PhantomData,
        }
    }

    pub fn map_region_at_addr<SizeBits: Unsigned, SS: SharedStatus>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, SS>,
        vaddr: usize,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<
        MappedMemoryRegion<SizeBits, SS>,
        (VSpaceError, Option<UnmappedMemoryRegion<SizeBits, SS>>),
    >
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        match self.weak_map_region_at_addr(region.weaken(), vaddr, rights, vm_attributes) {
            Ok(r) => Ok(r.as_strong::<SizeBits>().map_err(|e| (e, None))?),
            Err((e, r)) => Err((e, r.as_strong::<SizeBits>().ok())),
        }
    }

    pub fn weak_map_region_at_addr<SS: SharedStatus>(
        &mut self,
        region: WeakUnmappedMemoryRegion<SS>,
        vaddr: usize,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<WeakMappedMemoryRegion<SS>, (VSpaceError, WeakUnmappedMemoryRegion<SS>)> {
        if region.size_bits() < PageBits::U8 {
            return Err((VSpaceError::InvalidRegionSize, region));
        }

        // Verify that we can fit this region into the address space.
        match vaddr.checked_add(region.size_bytes()) {
            None => return Err((VSpaceError::ExceededAddressableSpace, region)),
            _ => (),
        };

        let mut mapping_vaddr = vaddr;
        let cptr = region.caps.start_cptr;
        let slot_count = region.caps.len;
        let size_bits = region.size_bits();

        // N.B. Currently expect a single continuous cap range of all pages.
        let mut mapped_granules: Option<WeakCapRange<Page<granule_state::Mapped>, role::Local>> =
            None;

        fn unmap_mapped_granule_cptrs(
            mapped_granules: Option<WeakCapRange<Granule<granule_state::Mapped>, role::Local>>,
        ) -> Result<(), SeL4Error> {
            if let Some(mapped_granule) = mapped_granules {
                mapped_granules
                    .into_iter()
                    .map(|granule| granule.unmap().map(|_p| ()))
                    .collect()
            } else {
                Ok(())
            }
        }
        let kind = region.kind;

        for granule in region.caps.into_iter() {
            match self.layers.map_layer(
                &granule,
                mapping_vaddr,
                &mut self.root,
                rights,
                vm_attributes,
                &mut self.untyped,
                &mut self.slots,
            ) {
                Err(MappingError::PageMapFailure(e))
                | Err(MappingError::IntermediateLayerFailure(e)) => {
                    // Rollback the pages we've mapped thus far.
                    let _ = unmap_mapped_granule_cptrs(mapped_granules);
                    return Err((
                        VSpaceError::SeL4Error(e),
                        WeakMemoryRegion::unchecked_new(
                            cptr,
                            slot_count,
                            granule_state::Unmapped,
                            kind,
                            size_bits,
                        ),
                    ));
                }
                Err(e) => {
                    // Rollback the pages we've mapped thus far.
                    let _ = unmap_mapped_granule_cptrs(mapped_granules);
                    return Err((
                        VSpaceError::MappingError(e),
                        WeakMemoryRegion::unchecked_new(
                            cptr,
                            slot_count,
                            granule_state::Unmapped,
                            kind,
                            size_bits,
                        ),
                    ));
                }
                Ok(_) => {
                    // save pages we've mapped thus far so we can roll
                    // them back if we fail to map all of this
                    // region. I.e., something was previously mapped
                    // here.
                    match mapped_granules {
                        None => {
                            mapped_granules = Some(WeakCapRange::new(
                                granule.cptr,
                                Page {
                                    state: granule_state::Mapped {
                                        vaddr: mapping_vaddr,
                                        asid: self.asid(),
                                    },
                                },
                                1,
                            ));
                        }
                        Some(mut already_mapped_granules) => {
                            already_mapped_granules.len += 1;
                            mapped_granules = Some(already_mapped_granules)
                        }
                    }
                }
            };
            mapping_vaddr += granule.cap_data.size_bytes();
        }

        if let Err(e) = self
            .available_address_range
            .observe_mapping(vaddr, size_bits)
        {
            // Rollback the pages we've mapped thus far.
            let _ = unmap_mapped_granule_cptrs(mapped_granules);
            return Err((
                e,
                WeakMemoryRegion::unchecked_new(
                    cptr,
                    slot_count,
                    granule_state::Unmapped,
                    kind,
                    size_bits,
                ),
            ));
        }

        Ok(WeakMappedMemoryRegion::unchecked_new(
            cptr,
            slot_count,
            granule_state::Mapped {
                vaddr,
                asid: self.asid,
            },
            kind,
            size_bits,
        ))
    }

    /// Map a region of memory at some address, I don't care where.
    pub fn map_region<SizeBits: Unsigned>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Exclusive>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.map_region_internal(region, rights, vm_attributes)
    }

    /// Map a weak region of memory at some address, I don't care where.
    pub fn weak_map_region(
        &mut self,
        region: WeakUnmappedMemoryRegion<shared_status::Exclusive>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<WeakMappedMemoryRegion<shared_status::Exclusive>, VSpaceError> {
        self.weak_map_region_internal(region, rights, vm_attributes)
    }

    /// Map a region of memory at some address, then move it to a
    /// different cspace.
    pub fn map_region_and_move<SizeBits: Unsigned, Role: CNodeRole>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slots: CNodeSlots<GranuleSlotCount<SizeBits>, Role>,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Exclusive>, VSpaceError> {
        self.weak_map_region_and_move(
            region.weaken(),
            rights,
            vm_attributes,
            src_cnode,
            &mut dest_slots.weaken(),
        )
        .and_then(|r| r.as_strong::<SizeBits>())
    }

    /// Map a weak region of memory at some address, then move it to a
    /// different cspace.
    pub fn weak_map_region_and_move<Role: CNodeRole>(
        &mut self,
        region: WeakUnmappedMemoryRegion<shared_status::Exclusive>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slots: &mut LocalCap<WCNodeSlotsData<Role>>,
    ) -> Result<WeakMappedMemoryRegion<shared_status::Exclusive>, VSpaceError> {
        let gran_info = arch::determine_best_granule_fit(region.size_bits);
        if dest_slots.size() < gran_info.count() {
            return Err(VSpaceError::InsufficientCNodeSlots);
        }
        let kind = region.kind;
        let size_bits = region.size_bits();
        let mapped_region: WeakMappedMemoryRegion<shared_status::Exclusive> =
            self.weak_map_region_internal(region, rights, vm_attributes)?;
        let vaddr = mapped_region.vaddr();
        let slot_count = mapped_region.caps.len;
        let dest_init_cptr = dest_slots.cap_data.offset;

        for (page, slot) in mapped_region
            .caps
            .into_iter()
            .zip(dest_slots.incrementally_consuming_iter())
        {
            let _ = page.move_to_slot(src_cnode, slot)?;
        }

        Ok(WeakMappedMemoryRegion::unchecked_new(
            dest_init_cptr,
            slot_count,
            granule_state::Mapped {
                vaddr,
                asid: self.asid,
            },
            kind,
            size_bits,
        ))
    }

    /// Map a _shared_ region of memory at some address, I don't care
    /// where. When `map_shared_region` is called, the caps making up
    /// this region are copied using the slots and cnode provided.
    /// The incoming `UnmappedMemoryRegion` is only borrowed and one
    /// also gets back a new `MappedMemoryRegion` indexed with the
    /// status `Shared`.
    pub fn map_shared_region<SizeBits: Unsigned>(
        &mut self,
        region: &UnmappedMemoryRegion<SizeBits, shared_status::Shared>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        slots: LocalCNodeSlots<WorstCaseGranulesSlots>,
        cnode: &LocalCap<LocalCNode>,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Shared>, VSpaceError> {
        let unmapped_sr: UnmappedMemoryRegion<_, shared_status::Shared> =
            UnmappedMemoryRegion::from_caps(region.caps.copy(cnode, slots, rights)?, region.kind);
        self.map_region_internal(unmapped_sr, rights, vm_attributes)
    }
    /// Map a _shared_ region of memory at some address, I don't care
    /// where. When `map_shared_region` is called, the caps making up
    /// this region are copied using the slots and cnode provided.
    /// The incoming `UnmappedMemoryRegion` is only borrowed and one
    /// also gets back a new `MappedMemoryRegion` indexed with the
    /// status `Shared`.
    pub fn weak_map_shared_region(
        &mut self,
        region: &WeakUnmappedMemoryRegion<shared_status::Shared>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        slots: &mut LocalCap<WCNodeSlotsData<role::Local>>,
        cnode: &LocalCap<LocalCNode>,
    ) -> Result<WeakMappedMemoryRegion<shared_status::Shared>, VSpaceError> {
        let caps_copy = region
            .caps
            .copy(cnode, slots, rights)
            .map_err(|e| match e {
                WeakCopyError::NotEnoughSlots => VSpaceError::InsufficientCNodeSlots,
                WeakCopyError::SeL4Error(e) => VSpaceError::SeL4Error(e),
            })?;
        let unmapped_sr: WeakUnmappedMemoryRegion<shared_status::Shared> =
            WeakMemoryRegion::try_from_caps(caps_copy, region.kind, region.size_bits())
                .map_err(|_| VSpaceError::InvalidRegionSize)?;
        self.weak_map_region_internal(unmapped_sr, rights, vm_attributes)
    }

    /// For cases when one does not want to continue to duplicate the
    /// region's constituent caps—meaning that there is only one final
    /// address space in which this region will be mapped—that
    /// unmapped region can be consumed and a mapped region is
    /// returned.
    pub fn map_shared_region_and_consume<SizeBits: Unsigned>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Shared>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Shared>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.map_region_internal(region, rights, vm_attributes)
    }

    fn map_region_internal<SizeBits: Unsigned, SSIn: SharedStatus, SSOut: SharedStatus>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, SSIn>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<MappedMemoryRegion<SizeBits, SSOut>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.weak_map_region_internal(region.weaken(), rights, vm_attributes)
            .and_then(|r| r.as_strong::<SizeBits>())
    }
    fn weak_map_region_internal<SSIn: SharedStatus, SSOut: SharedStatus>(
        &mut self,
        region: WeakUnmappedMemoryRegion<SSIn>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<WeakMappedMemoryRegion<SSOut>, VSpaceError> {
        let starting_address = self
            .available_address_range
            .auto_propose_region_start(region.size_bits())
            .map_err(|_| VSpaceError::InsufficientAddressSpaceAvailableToMapRegion)?;

        // create the mapped region first because we need to pluck out
        // the `start_cptr` before the iteration below consumes the
        // unmapped region.
        let mapped_region = WeakMappedMemoryRegion::unchecked_new(
            region.caps.start_cptr,
            region.caps.len,
            granule_state::Mapped {
                vaddr: starting_address,
                asid: self.asid(),
            },
            region.kind,
            region.size_bits(),
        );

        let mut vaddr = starting_address;
        for page_cap in region.caps.into_iter() {
            match self.layers.map_layer(
                &page_cap,
                vaddr,
                &mut self.root,
                rights,
                vm_attributes,
                &mut self.untyped,
                &mut self.slots,
            ) {
                Err(MappingError::PageMapFailure(e))
                | Err(MappingError::IntermediateLayerFailure(e)) => {
                    return Err(VSpaceError::SeL4Error(e))
                }
                Err(e) => return Err(VSpaceError::MappingError(e)),
                Ok(_) => self
                    .available_address_range
                    .observe_mapping(vaddr, PageBits::U8)?,
            };
            // It's safe to do a direct addition as we've already
            // determined that this region will fit here.
            vaddr += PageBytes::USIZE;
        }

        Ok(mapped_region)
    }

    pub(crate) fn skip_pages(&mut self, count: usize) -> Result<(), VSpaceError> {
        for _ in 0..count {
            let starting_address = self
                .available_address_range
                .auto_propose_region_start(PageBits::U8)
                .map_err(|_| VSpaceError::ExceededAddressableSpace)?;
            self.available_address_range
                .observe_mapping(starting_address, PageBits::U8)?;
        }
        Ok(())
    }
}

/// A dual-cursor address range tracker that maintains
/// watermarks tracking an unallocated middle-region.
#[derive(Debug, Clone)]
struct AvailableAddressRange {
    /// Watermark for the lowest starting address available
    bottom: usize,
    /// Watermark for the highest ending address available
    top: usize,
}

impl Default for AvailableAddressRange {
    fn default() -> Self {
        AvailableAddressRange {
            bottom: 0,
            top: core::usize::MAX,
        }
    }
}

impl AvailableAddressRange {
    fn observe_mapping(&mut self, start: usize, size_bits: u8) -> Result<(), VSpaceError> {
        let size_bytes = bytes_from_size_bits(size_bits);
        let end = start
            .checked_add(size_bytes)
            .ok_or_else(|| VSpaceError::ExceededAddressableSpace)?;
        if end < self.bottom || start > self.top {
            return Ok(());
        }

        let distance_from_top = self.top - start;
        let distance_from_bottom = end - self.bottom;
        if distance_from_bottom < distance_from_top {
            self.bottom = core::cmp::max(self.bottom, end);
        } else {
            self.top = core::cmp::min(self.top, start);
        }
        Ok(())
    }

    fn auto_propose_region_start(&self, size_bits: u8) -> Result<usize, CouldNotAllocateRegion> {
        if self.bottom > self.top {
            return Err(CouldNotAllocateRegion);
        }
        let size_bytes = bytes_from_size_bits(size_bits);
        let proposed_start = self.bottom;
        let proposed_end = proposed_start
            .checked_add(size_bytes)
            .ok_or_else(|| CouldNotAllocateRegion)?;
        if proposed_end > self.top {
            return Err(CouldNotAllocateRegion);
        }
        Ok(proposed_start)
    }
}

struct CouldNotAllocateRegion;

fn bytes_from_size_bits(size_bits: u8) -> usize {
    2usize.pow(u32::from(size_bits))
}

mod private {
    use super::vspace_state::{Empty, Imaged};
    pub trait SealedVSpaceState {}
    impl SealedVSpaceState for Empty {}
    impl SealedVSpaceState for Imaged {}
}
