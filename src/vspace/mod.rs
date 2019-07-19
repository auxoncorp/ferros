//! A VSpace represents the virtual address space of a process in
//! seL4.
//!
//! This architecture-independent realization of that concept uses
//! memory _regions_ rather than expose the granules that each layer
//! in the addressing structures is responsible for mapping.
use core::marker::PhantomData;
use core::ops::Sub;

use arrayvec::{ArrayVec, CapacityError};
use typenum::*;

use crate::alloc::ut_buddy::{self, UTBuddyError, WUTBuddy};
use crate::arch::cap::{page_state, AssignedASID, Page, UnassignedASID};
use crate::arch::{self, AddressSpace, PageBits, PageBytes, PagingRoot};
use crate::bootstrap::UserImage;
use crate::cap::{
    memory_kind, role, CNodeRole, CNodeSlots, Cap, CapRange, CapRangeDataReconstruction, CapType,
    ChildCNodeSlot, DirectRetype, InternalASID, LocalCNode, LocalCNodeSlots, LocalCap, PhantomCap,
    RetypeError, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped, WeakCapRange,
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
    ExceededAvailableAddressSpace,
    ASIDMismatch,
    OverlappingRegion,
    OutOfRegions,

    /// This error is returned by `map_region_at_addr` its rollback
    /// ArrayVec is not large enough to hold the number of pages, it's
    /// arbitrary and we'll need to address this when we get to doing
    /// special-sized granules.
    TriedToMapTooManyPagesAtOnce,
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

impl From<CapacityError<(usize, usize)>> for VSpaceError {
    fn from(_: CapacityError<(usize, usize)>) -> VSpaceError {
        VSpaceError::OutOfRegions
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

// 2^12 / PageCount
pub(crate) type NumPages<Size> = Pow<op!(Size - PageBits)>;

pub enum ProcessCodeImageConfig<'a, 'b, 'c> {
    ReadOnly,
    /// Use when you need to be able to write to statics in the child process
    ReadWritable {
        parent_vspace_scratch: &'a mut ScratchRegion<'b, 'c>,
        code_pages_ut: LocalCap<Untyped<crate::arch::TotalCodeSizeBits>>,
        code_pages_slots: LocalCNodeSlots<crate::arch::CodePageCount>,
    },
}

const NUM_SPECIFIC_REGIONS: usize = 128;

struct RegionLocations {
    regions: ArrayVec<[(usize, usize); NUM_SPECIFIC_REGIONS]>,
}

impl RegionLocations {
    fn new() -> Self {
        RegionLocations {
            regions: ArrayVec::new(),
        }
    }

    fn add<SizeBits: Unsigned, SS: SharedStatus>(
        &mut self,
        region: &MappedMemoryRegion<SizeBits, SS>,
    ) -> Result<(), VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        let regions_len = self.regions.len();
        let index = {
            let mut idx = 0;
            for i in 0..regions_len {
                // We've reached the tail (len - 1), or this is the
                // initial insertion (len == i == 0).
                if i == regions_len - 1 || i == regions_len {
                    idx = regions_len;
                    break;
                }

                let (addr, _) = self.regions[i];
                if region.vaddr() < addr {
                    idx = i;
                    break;
                }
            }
            idx
        };

        // This new region is either greater than all the others or is
        // the first to be added so we'll just push it at the
        // tail.
        if index == regions_len {
            return self
                .regions
                .try_push((region.vaddr(), region.size()))
                .map_err(|_| VSpaceError::OutOfRegions);
        }

        // We want to check beforehand whether or not we can do the
        // insert, otherwise we're stuck with a mutated `self.regions`
        // and are left to put the peices back as we found them.
        if self.regions.len() == self.regions.capacity() {
            return Err(VSpaceError::OutOfRegions);
        }

        self.regions.insert(index, (region.vaddr(), region.size()));

        Ok(())
    }

    fn is_overlap(&self, desired_vaddr: usize) -> bool {
        self.regions.iter().any(|(addr, size)| {
            if desired_vaddr >= *addr && desired_vaddr < (*addr + size) {
                return true;
            }
            false
        })
    }

    fn find_first_fit<SizeBits: Unsigned, SS: SharedStatus>(
        &self,
        current_addr: usize,
        desired_region: &UnmappedMemoryRegion<SizeBits, SS>,
    ) -> Result<usize, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        struct FoldState {
            found: bool,
            current_addr: usize,
        }

        let fit = self.regions.iter().fold(
            Ok(FoldState {
                found: false,
                current_addr,
            }),
            |fold_state, (region_addr, region_size)| {
                if let Ok(fs) = fold_state {
                    // We've found a chunk of address space that can
                    // fit this region. Just carry through our result.
                    if fs.found {
                        return Ok(fs);
                    }

                    // If our cursor + the desired_region's size is
                    // less than the this region's address then we can
                    // fit this region in this chunk. Set `found` to
                    // true and retain the current address.
                    if fs.current_addr + desired_region.size() < *region_addr {
                        return Ok(FoldState {
                            found: true,
                            current_addr: fs.current_addr,
                        });
                    }

                    // Otherwise, skip forward to the end of the
                    // region at hand. If we run out of address space,
                    // say so.
                    let next_addr = match region_addr.checked_add(*region_size) {
                        Some(n) => n,
                        None => return Err(VSpaceError::ExceededAvailableAddressSpace),
                    };
                    return Ok(FoldState {
                        found: false,
                        current_addr: next_addr,
                    });
                }

                // We've encountered an error! Do no further
                // processing and just return the error.
                fold_state
            },
        )?;
        Ok(fit.current_addr)
    }
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
    /// When a map request comes in which does not target a specific
    /// address, this helps the VSpace decide where to put that
    /// region.
    next_addr: usize,
    /// The following two members are the resources used by the VSpace
    /// when building out intermediate layers.
    untyped: WUTBuddy<CapRole>,
    slots: Cap<WCNodeSlotsData<CapRole>, CapRole>,
    specific_regions: RegionLocations,
    _state: PhantomData<State>,
}

impl VSpace<vspace_state::Empty> {
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
            next_addr: 0,
            untyped: ut_buddy::weak_ut_buddy(untyped),
            slots,
            specific_regions: RegionLocations::new(),
            _state: PhantomData,
        })
    }
}

impl<S: VSpaceState> VSpace<S> {
    /// This address space's id.
    pub(crate) fn asid(&self) -> InternalASID {
        self.asid
    }

    /// Map a given page at some address, I don't care where.
    ///
    /// Note: Generally, we should be operating on regions, but in the
    /// case of the system call for configuring a TCB, a mapped page's
    /// vaddr and its cap must be provided. To obfuscate these behind
    /// a region seems unnecessary. Therefore we provide a
    /// method to talk about mapping only a page.
    pub fn map_given_page(
        &mut self,
        page: LocalCap<Page<page_state::Unmapped>>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
    ) -> Result<LocalCap<Page<page_state::Mapped>>, VSpaceError> {
        let future_next_addr = match self.next_addr.checked_add(PageBytes::USIZE) {
            Some(n) => n,
            None => return Err(VSpaceError::ExceededAvailableAddressSpace),
        };

        match self.layers.map_layer(
            &page,
            self.next_addr,
            &mut self.root,
            rights,
            vm_attributes,
            &mut self.untyped,
            &mut self.slots,
        ) {
            Err(MappingError::PageMapFailure(e)) => return Err(VSpaceError::SeL4Error(e)),
            Err(MappingError::IntermediateLayerFailure(e)) => {
                return Err(VSpaceError::SeL4Error(e));
            }
            Err(e) => return Err(VSpaceError::MappingError(e)),
            Ok(_) => (),
        };

        let vaddr = self.next_addr;
        self.next_addr = future_next_addr;

        Ok(Cap {
            cptr: page.cptr,
            cap_data: Page {
                state: page_state::Mapped {
                    asid: self.asid(),
                    vaddr,
                },
            },
            _role: PhantomData,
        })
    }
}

impl VSpace<vspace_state::Imaged> {
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
        // NB: For now, we make use of a constant program start address, but we expect
        // this to become dynamic as application framework based inspection
        // and dynamic representation of the code images advances.
        vspace.next_addr = crate::arch::ProgramStart::USIZE;
        match code_image_config {
            ProcessCodeImageConfig::ReadOnly => {
                for (page_cap, slot) in user_image.pages_iter().zip(code_slots.into_strong_iter()) {
                    let copied_page_cap = page_cap.copy(&parent_cnode, slot, CapRights::R)?;
                    let _ = vspace.map_given_page(
                        copied_page_cap,
                        CapRights::R,
                        arch::vm_attributes::DEFAULT,
                    )?;
                }
            }
            ProcessCodeImageConfig::ReadWritable {
                parent_vspace_scratch,
                code_pages_ut,
                code_pages_slots,
            } => {
                // First, retype the untyped into `CodePageCount`
                // pages.
                let fresh_pages: CapRange<
                    Page<page_state::Unmapped>,
                    role::Local,
                    arch::CodePageCount,
                > = code_pages_ut.retype_multi(code_pages_slots)?;
                // Then, zip up the pages with the user image pages
                for (ui_page, fresh_page) in user_image.pages_iter().zip(fresh_pages.iter()) {
                    // Temporarily map the new page and copy the data
                    // from `user_image` to the new page.
                    let mut unmapped_region = fresh_page.to_region();
                    let _ = parent_vspace_scratch.temporarily_map_region::<PageBits, _, _>(
                        &mut unmapped_region,
                        |temp_mapped_region| {
                            unsafe {
                                *(core::mem::transmute::<usize, *mut [usize; arch::WORDS_PER_PAGE]>(
                                    temp_mapped_region.vaddr(),
                                )) = *(core::mem::transmute::<
                                    usize,
                                    *const [usize; arch::WORDS_PER_PAGE],
                                >(
                                    ui_page.cap_data.state.vaddr
                                ))
                            };
                        },
                    )?;
                    // Finally, map that page into the target vspace
                    // N.B. This mapping assumes that the provided UserImage
                    // reserves a single contiguous region after some starting offset
                    // and that the VSpace has been mutated to match that starting offset
                    // and that we always copy-map pages without rearrangement or skipping
                    let _mapped_page = vspace.map_given_page(
                        unmapped_region.to_page(),
                        CapRights::RW,
                        arch::vm_attributes::DEFAULT,
                    )?;
                }
            }
        }

        Ok(VSpace {
            root: vspace.root,
            asid: vspace.asid,
            layers: vspace.layers,
            next_addr: vspace.next_addr,
            untyped: vspace.untyped,
            slots: vspace.slots,
            specific_regions: RegionLocations::new(),
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
        VSpace {
            layers: AddressSpace::new(),
            root: Cap {
                cptr: root_vspace_cptr,
                cap_data: PagingRoot::phantom_instance(),
                _role: PhantomData,
            },
            untyped: ut_buddy::weak_ut_buddy(ut),
            slots: cslots,
            specific_regions: RegionLocations::new(),
            next_addr,
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
    ) -> Result<MappedMemoryRegion<SizeBits, SS>, (VSpaceError, UnmappedMemoryRegion<SizeBits, SS>)>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        if self.specific_regions.is_overlap(vaddr) {
            return Err((VSpaceError::OverlappingRegion, region));
        }

        // Verify that we can fit this region into the address space.
        match vaddr.checked_add(region.size()) {
            None => return Err((VSpaceError::ExceededAvailableAddressSpace, region)),
            _ => (),
        };

        let mut mapping_vaddr = vaddr;
        let cptr = region.caps.start_cptr;

        let mut mapped_pages_cptrs = util::WeakCapRangeCollection::new();

        impl CapRangeDataReconstruction for Page<page_state::Mapped> {
            fn reconstruct(index: usize, seed_cap_data: &Self) -> Self {
                Page {
                    state: page_state::Mapped {
                        vaddr: seed_cap_data.state.vaddr + index * PageBytes::USIZE,
                        asid: seed_cap_data.state.asid,
                    },
                }
            }
        }
        fn unmap_mapped_page_cptrs(
            mapped_pages: util::WeakCapRangeCollection<Page<page_state::Mapped>>,
        ) -> Result<(), SeL4Error> {
            mapped_pages
                .into_iter()
                .map(|page| page.unmap().map(|_p| ()))
                .collect()
        }

        for page in region.caps.iter() {
            match self.layers.map_layer(
                &page,
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
                    let _ = unmap_mapped_page_cptrs(mapped_pages_cptrs);
                    return Err((
                        VSpaceError::SeL4Error(e),
                        UnmappedMemoryRegion::unchecked_new(cptr),
                    ));
                }
                Err(e) => {
                    // Rollback the pages we've mapped thus far.
                    let _ = unmap_mapped_page_cptrs(mapped_pages_cptrs);
                    return Err((
                        VSpaceError::MappingError(e),
                        UnmappedMemoryRegion::unchecked_new(cptr),
                    ));
                }
                Ok(_) => {
                    // save pages we've mapped thus far so we can roll
                    // them back if we fail to map all of this
                    // region. I.e., something was previously mapped
                    // here.
                    match mapped_pages_cptrs.try_push(Cap {
                        cptr: page.cptr,
                        cap_data: Page {
                            state: page_state::Mapped {
                                vaddr: mapping_vaddr,
                                asid: self.asid,
                            },
                        },
                        _role: PhantomData,
                    }) {
                        Err(_) => {
                            return Err((
                                VSpaceError::TriedToMapTooManyPagesAtOnce,
                                UnmappedMemoryRegion::unchecked_new(cptr),
                            ))
                        }
                        _ => (),
                    }
                }
            };
            mapping_vaddr += PageBytes::USIZE;
        }

        let region = MappedMemoryRegion::unchecked_new(cptr, vaddr, self.asid);

        match self.specific_regions.add(&region) {
            Err(_) => {
                let _ = unmap_mapped_page_cptrs(mapped_pages_cptrs);
                Err((
                    VSpaceError::OutOfRegions,
                    UnmappedMemoryRegion::unchecked_new(cptr),
                ))
            }
            Ok(_) => Ok(region),
        }
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

    /// Map a region of memory at some address, then move it to a
    /// different cspace.
    pub fn map_region_and_move<SizeBits: Unsigned, Role: CNodeRole>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        rights: CapRights,
        vm_attributes: arch::VMAttributes,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slots: CNodeSlots<NumPages<SizeBits>, Role>,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Exclusive>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        let mapped_region: MappedMemoryRegion<_, shared_status::Exclusive> =
            self.map_region_internal(region, rights, vm_attributes)?;
        let vaddr = mapped_region.vaddr();
        let dest_init_cptr = dest_slots.cap_data.offset;

        for (page, slot) in mapped_region.caps.iter().zip(dest_slots.iter()) {
            let _ = page.move_to_slot(src_cnode, slot)?;
        }

        Ok(MappedMemoryRegion::unchecked_new(
            dest_init_cptr,
            vaddr,
            self.asid,
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
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
        cnode: &LocalCap<LocalCNode>,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Shared>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        let unmapped_sr: UnmappedMemoryRegion<_, shared_status::Shared> =
            UnmappedMemoryRegion::from_caps(region.caps.copy(cnode, slots, rights)?);
        self.map_region_internal(unmapped_sr, rights, vm_attributes)
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

    /// Unmap a region.
    pub fn unmap_region<SizeBits: Unsigned, SS: SharedStatus>(
        &mut self,
        region: MappedMemoryRegion<SizeBits, SS>,
    ) -> Result<UnmappedMemoryRegion<SizeBits, SS>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        let start_cptr = region.caps.initial_cptr;
        for page_cap in region.caps.iter() {
            let _ = self.unmap_page(page_cap)?;
        }
        Ok(UnmappedMemoryRegion::unchecked_new(start_cptr))
    }

    pub(crate) fn root_cptr(&self) -> usize {
        self.root.cptr
    }

    fn unmap_page(
        &mut self,
        page: LocalCap<Page<page_state::Mapped>>,
    ) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error> {
        page.unmap()
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
        let mut vaddr = self.find_next_vaddr(&region)?;

        let future_next_addr = match vaddr.checked_add(region.size()) {
            Some(n) => n,
            None => return Err(VSpaceError::ExceededAvailableAddressSpace),
        };

        // create the mapped region first because we need to pluck out
        // the `start_cptr` before the iteration below consumes the
        // unmapped region.
        let mapped_region =
            MappedMemoryRegion::unchecked_new(region.caps.start_cptr, vaddr, self.asid());

        for page_cap in region.caps.iter() {
            match self.layers.map_layer(
                &page_cap,
                vaddr,
                &mut self.root,
                rights,
                vm_attributes,
                &mut self.untyped,
                &mut self.slots,
            ) {
                Err(MappingError::PageMapFailure(e)) => return Err(VSpaceError::SeL4Error(e)),
                Err(MappingError::IntermediateLayerFailure(e)) => {
                    return Err(VSpaceError::SeL4Error(e));
                }
                Err(e) => return Err(VSpaceError::MappingError(e)),
                Ok(_) => (),
            };
            // It's safe to do a direct addition as we've already
            // determined that this region will fit here.
            vaddr += PageBytes::USIZE;
        }

        self.next_addr = future_next_addr;

        Ok(mapped_region)
    }

    fn find_next_vaddr<SizeBits: Unsigned, SS: SharedStatus>(
        &self,
        region: &UnmappedMemoryRegion<SizeBits, SS>,
    ) -> Result<usize, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.specific_regions.find_first_fit(self.next_addr, region)
    }

    pub(crate) fn skip_pages(&mut self, count: usize) -> Result<(), VSpaceError> {
        if let Some(next) = PageBytes::USIZE
            .checked_mul(count)
            .and_then(|bytes| self.next_addr.checked_add(bytes))
        {
            self.next_addr = next;
            Ok(())
        } else {
            Err(VSpaceError::ExceededAvailableAddressSpace)
        }
    }

    pub fn reserve<PageCount: Unsigned>(
        &mut self,
        sacrificial_page: LocalCap<Page<page_state::Unmapped>>,
    ) -> Result<ReservedRegion<PageCount>, VSpaceError>
    where
        PageCount: IsGreaterOrEqual<U1, Output = True>,
    {
        ReservedRegion::new(self, sacrificial_page)
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
            next_addr,
            untyped,
            slots: _,
            specific_regions,
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
            next_addr,
            untyped: child_untyped,
            slots: child_paging_slots,
            specific_regions,
            _state: PhantomData,
        })
    }
}

/// A region of memory in a VSpace that has been reserved
/// for future scratch-style/temporary usage.
///
/// Its backing paging structures have all been pre-created,
/// so mapping individual pages to this region should require
/// no overhead resources whatsoever.
///
/// Note that the type parameter regarding default size matches
/// the currently defaulted number of pages allowed for a process
/// stack.
pub struct ReservedRegion<PageCount: Unsigned = crate::userland::process::DefaultStackPageCount> {
    vaddr: usize,
    asid: InternalASID,
    _page_count: PhantomData<PageCount>,
}

impl<PageCount: Unsigned> ReservedRegion<PageCount>
where
    PageCount: IsGreaterOrEqual<U1, Output = True>,
{
    pub fn size(&self) -> usize {
        PageCount::USIZE * crate::arch::PageBytes::USIZE
    }

    pub fn new(
        vspace: &mut VSpace,
        sacrificial_page: LocalCap<Page<page_state::Unmapped>>,
    ) -> Result<Self, VSpaceError> {
        let mut unmapped_page = sacrificial_page;
        let mut first_vaddr = None;
        // Map (and then unmap) each page in the reserved range
        // in order to trigger the instantiation of the backing paging
        // structures.
        for _ in 0..PageCount::USIZE {
            let mapped_page = vspace.map_given_page(
                unmapped_page,
                CapRights::RW,
                arch::vm_attributes::DEFAULT,
            )?;
            if let None = first_vaddr {
                first_vaddr = Some(mapped_page.cap_data.state.vaddr);
            }
            unmapped_page = vspace.unmap_page(mapped_page)?;
        }
        Ok(ReservedRegion {
            // Due to the type constraint that ensures PageCount > 0, this must be Some
            vaddr: first_vaddr.unwrap(),
            asid: vspace.asid(),
            _page_count: PhantomData,
        })
    }

    pub fn as_scratch<'a, 'b>(
        &'a self,
        vspace: &'b mut VSpace,
    ) -> Result<ScratchRegion<'a, 'b, PageCount>, VSpaceError> {
        ScratchRegion::new(self, vspace)
    }
}

/// Borrow of a reserved region and its associated VSpace in order to support
/// temporary mapping
pub struct ScratchRegion<
    'a,
    'b,
    PageCount: Unsigned = crate::userland::process::DefaultStackPageCount,
> {
    reserved_region: &'a ReservedRegion<PageCount>,
    vspace: &'b mut VSpace,
}

impl<'a, 'b, PageCount: Unsigned> ScratchRegion<'a, 'b, PageCount>
where
    PageCount: IsGreaterOrEqual<U1, Output = True>,
{
    pub fn new(
        region: &'a ReservedRegion<PageCount>,
        vspace: &'b mut VSpace,
    ) -> Result<Self, VSpaceError> {
        if region.asid == vspace.asid() {
            Ok(ScratchRegion {
                reserved_region: region,
                vspace,
            })
        } else {
            Err(VSpaceError::ASIDMismatch)
        }
    }

    // TODO - add more safety rails to prevent returning something from the
    // inner function that becomes invalid when the page is unmapped locally
    //
    /// Map a region temporarily and do with it as thou wilt with `f`.
    ///
    /// Note that this is defined on a region which has the shared
    /// status of `Exclusive`. The idea here is to do the initial
    /// region-filling work with `temporarily_map_region` _before_
    /// sharing this page and mapping it into other address
    /// spaces. This enforced order ought to prevent one from
    /// forgetting to do the region-filling initialization.
    pub fn temporarily_map_region<SizeBits: Unsigned, F, Out>(
        &mut self,
        region: &mut UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        f: F,
    ) -> Result<Out, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
        F: Fn(&mut MappedMemoryRegion<SizeBits, shared_status::Exclusive>) -> Out,
        PageCount: IsGreaterOrEqual<NumPages<SizeBits>, Output = True>,
        PageCount: IsGreaterOrEqual<U1, Output = True>,
    {
        let start_vaddr = self.reserved_region.vaddr;

        fn dummy_empty_slots() -> WCNodeSlots {
            // NB: Not happy with this fake cptr,
            // at least we can expect it to blow up loudly
            Cap {
                cptr: core::usize::MAX,
                _role: PhantomData,
                cap_data: WCNodeSlotsData {
                    offset: 0,
                    size: 0,
                    _role: PhantomData,
                },
            }
        }
        let unmapped_region_copy: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive> =
            UnmappedMemoryRegion::unchecked_new(region.caps.start_cptr);
        let mut next_addr = start_vaddr;
        for page in unmapped_region_copy.caps.iter() {
            match self.vspace.layers.map_layer(
                &page,
                next_addr,
                &mut self.vspace.root,
                CapRights::RW,
                arch::vm_attributes::DEFAULT,
                // NB: In the case of a ReservedRegion, we've already
                // mapped any of the intermediate layers so should
                // therefore not need a cache of resources for
                // temporarily mapping this scratch region.
                &mut WUTBuddy::empty(),
                &mut dummy_empty_slots(),
            ) {
                Err(MappingError::PageMapFailure(e)) => return Err(VSpaceError::SeL4Error(e)),
                Err(MappingError::IntermediateLayerFailure(e)) => {
                    return Err(VSpaceError::SeL4Error(e));
                }
                Err(e) => return Err(VSpaceError::MappingError(e)),
                Ok(_) => (),
            };
            next_addr += PageBytes::USIZE;
        }
        // map the pages at our predetermined/pre-allocated vaddr range
        let mut mapped_region = MappedMemoryRegion::unchecked_new(
            region.caps.start_cptr,
            start_vaddr,
            self.reserved_region.asid,
        );
        let res = f(&mut mapped_region);
        let _ = self.vspace.unmap_region(mapped_region)?;
        Ok(res)
    }
}

mod util {
    use super::*;

    const MAX_DISCONTINUOUS_CPTR_RANGES: usize = 16;

    /// A slightly dense collection of ranges of cptrs
    /// Must only be used with cap-types that can be reconstructed from
    /// nothing or from a single seed starting cap_data for a range
    pub(super) struct WeakCapRangeCollection<CT: CapType> {
        ranges: ArrayVec<[WeakCapRange<CT, role::Local>; MAX_DISCONTINUOUS_CPTR_RANGES]>,
    }

    impl<CT: CapType> WeakCapRangeCollection<CT> {
        pub(super) fn new() -> Self {
            WeakCapRangeCollection {
                ranges: ArrayVec::new(),
            }
        }
        /// Add a single cap to the collection
        pub(super) fn try_push(
            &mut self,
            cap: LocalCap<CT>,
        ) -> Result<(), arrayvec::CapacityError<LocalCap<CT>>> {
            for r in self.ranges.iter_mut() {
                if cap.cptr == r.start_cptr + r.len + 1 {
                    r.len += 1;
                    return Ok(());
                }
            }
            self.ranges
                .try_push(WeakCapRange::new(cap.cptr, cap.cap_data, 1))
                .map_err(|e| {
                    let wcr = e.element();
                    arrayvec::CapacityError::new(Cap {
                        cptr: wcr.start_cptr,
                        cap_data: wcr.start_cap_data,
                        _role: PhantomData,
                    })
                })
        }

        /// Uses a function that accepts a cap-index (the index in the cap-range's iteration, NOT a cptr)
        /// and the seed (starting) cap_data to produce a new cap_data instance to produce the Cap item instances
        pub(crate) fn into_iter(self) -> impl Iterator<Item = LocalCap<CT>>
        where
            CT: CapRangeDataReconstruction,
        {
            self.ranges.into_iter().flat_map(move |r| r.into_iter())
        }
    }
}

mod private {
    use super::vspace_state::{Empty, Imaged};
    pub trait SealedVSpaceState {}
    impl SealedVSpaceState for Empty {}
    impl SealedVSpaceState for Imaged {}
}
