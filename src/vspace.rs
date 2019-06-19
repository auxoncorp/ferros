//! A VSpace represents the virtual address space of a process in
//! seL4.
//!
//! This architecture-independent realization of that concept uses
//! memory _regions_ rather than expose the granules that each layer
//! in the addressing structures is responsible for mapping.
use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use selfe_sys::*;

use crate::alloc::ut_buddy::{self, UTBuddyError, WUTBuddy};
use crate::arch::cap::{page_state, AssignedASID, Page, UnassignedASID};
use crate::arch::{self, AddressSpace, PageBits, PageBytes, PagingRoot};
use crate::bootstrap::UserImage;
use crate::cap::{
    role, CNodeRole, CNodeSlots, Cap, CapRange, CapType, DirectRetype, LocalCNode, LocalCNodeSlots,
    LocalCap, PhantomCap, RetypeError, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped,
};
use crate::error::SeL4Error;
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

pub trait VSpaceState: private::SealedVSpaceState {}

pub mod vspace_state {
    use super::VSpaceState;

    pub struct Empty;
    impl VSpaceState for Empty {}

    pub struct Imaged;
    impl VSpaceState for Imaged {}
}

/// A `Maps` implementor is a paging layer that maps granules of type
/// `G`. If this layer isn't present for the incoming address,
/// `MappingError::Overflow` should be returned, as this signals to
/// the caller—the layer above—that it needs to create a new object at
/// this layer and then attempt again to map the `item`.
pub trait Maps<G: CapType> {
    fn map_granule<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType;
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
    fn map_layer<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<Self::Item>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        utb: &mut WUTBuddy,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType;
}

/// `PagingTop` represents the root of an address space structure.
pub struct PagingTop<G, L: Maps<G>>
where
    L: CapType,
    G: CapType,
{
    pub layer: L,
    pub(super) _item: PhantomData<G>,
}

impl<G, L: Maps<G>> PagingLayer for PagingTop<G, L>
where
    L: CapType,
    G: CapType + DirectRetype + PhantomCap,
{
    type Item = G;
    fn map_layer<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        _utb: &mut WUTBuddy,
        _slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        self.layer.map_granule(item, addr, root, rights)
    }
}

/// `PagingRec` represents an intermediate layer. It is of type `L`,
/// while it maps `G`s. The layer above it is inside `P`.
pub struct PagingRec<G: CapType, L: Maps<G>, P: PagingLayer> {
    pub(crate) layer: L,
    pub(crate) next: P,
    pub(crate) _item: PhantomData<G>,
}

impl<G, L: Maps<G>, P: PagingLayer> PagingLayer for PagingRec<G, L, P>
where
    L: CapType,
    G: CapType + DirectRetype + PhantomCap,
{
    type Item = G;
    fn map_layer<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        utb: &mut WUTBuddy,
        mut slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        // Attempt to map this layer's granule.
        match self.layer.map_granule(item, addr, root, rights) {
            // if it fails with a lookup error, ask the next layer up
            // to map a new instance at this layer.
            Err(MappingError::Overflow) => {
                let mut ut = utb.alloc(slots, <P::Item as DirectRetype>::SizeBits::USIZE)?;
                let next_item = ut.retype::<P::Item>(&mut slots)?;
                self.next
                    .map_layer(&next_item, addr, root, rights, utb, slots)?;
                // Then try again to map this layer.
                self.layer.map_granule(item, addr, root, rights)
            }
            // Any other result (success \/ other failure cases) can
            // be returned as is.
            res => res,
        }
    }
}

// 2^12 * PageCount
type NumPages<Size> = Pow<op!(Size - PageBits)>;

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
    caps: CapRange<Page<page_state::Unmapped>, role::Local, NumPages<SizeBits>>,
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
    pub fn new<Role: CNodeRole>(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: CNodeSlots<NumPages<SizeBits>, Role>,
    ) -> Result<Self, VSpaceError> {
        let page_caps =
            ut.retype_multi_runtime::<Page<page_state::Unmapped>, NumPages<SizeBits>, _>(slots)?;
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

struct MappedPageRange<Count: Unsigned> {
    initial_cptr: usize,
    initial_vaddr: usize,
    asid: u32,
    _count: PhantomData<Count>,
}

impl<Count: Unsigned> MappedPageRange<Count> {
    fn new(initial_cptr: usize, initial_vaddr: usize, asid: u32) -> Self {
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
    caps: MappedPageRange<NumPages<SizeBits>>,
    asid: u32,
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

    pub(crate) fn size(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub(crate) fn vaddr(&self) -> usize {
        self.vaddr
    }

    pub fn share(
        self,
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
        cnode: &LocalCap<LocalCNode>,
        rights: CapRights,
    ) -> Result<
        (
            UnmappedMemoryRegion<SizeBits, shared_status::Shared>,
            MappedMemoryRegion<SizeBits, shared_status::Shared>,
        ),
        VSpaceError,
    > {
        let pages_offset = self.caps.initial_cptr;
        let vaddr = self.vaddr;
        let asid = self.asid;
        let slots_offset = slots.cap_data.offset;

        for (slot, page) in slots.iter().zip(self.caps.iter()) {
            page.copy(cnode, slot, rights)?;
        }

        Ok((
            UnmappedMemoryRegion {
                caps: CapRange::new(slots_offset),
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            MappedMemoryRegion::unchecked_new(pages_offset, vaddr, asid),
        ))
    }

    fn unchecked_new(
        initial_cptr: usize,
        initial_vaddr: usize,
        asid: u32,
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
    pub(crate) unsafe fn internal_alias(&mut self) -> Self {
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

pub enum ProcessCodeImageConfig<'a, 'b, 'c> {
    ReadOnly,
    /// Use when you need to be able to write to statics in the child process
    ReadWritable {
        parent_vspace_scratch: &'a mut ScratchRegion<'b, 'c>,
        code_pages_ut: LocalCap<Untyped<crate::arch::TotalCodeSizeBits>>,
        code_pages_slots: LocalCNodeSlots<crate::arch::CodePageCount>,
    },
}

/// A virtual address space manager.
pub struct VSpace<State: VSpaceState = vspace_state::Imaged> {
    /// The cap to this address space's root-of-the-tree item.
    root: LocalCap<PagingRoot>,
    /// The id of this address space.
    asid: LocalCap<AssignedASID>,
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
    untyped: WUTBuddy,
    slots: WCNodeSlots,
    _state: PhantomData<State>,
}

impl VSpace<vspace_state::Empty> {
    pub(crate) fn new(
        mut root_cap: LocalCap<PagingRoot>,
        asid: LocalCap<UnassignedASID>,
        slots: WCNodeSlots,
        untyped: LocalCap<WUntyped>,
    ) -> Result<Self, VSpaceError> {
        let assigned_asid = asid.assign(&mut root_cap)?;
        Ok(VSpace {
            root: root_cap,
            asid: assigned_asid,
            layers: AddressSpace::new(),
            next_addr: 0,
            untyped: ut_buddy::weak_ut_buddy(untyped),
            slots,
            _state: PhantomData,
        })
    }
}

impl<S: VSpaceState> VSpace<S> {
    /// This address space's id.
    pub(crate) fn asid(&self) -> u32 {
        self.asid.cap_data.asid
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
    ) -> Result<LocalCap<Page<page_state::Mapped>>, VSpaceError> {
        match self.layers.map_layer(
            &page,
            self.next_addr,
            &mut self.root,
            rights,
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
        self.next_addr += PageBytes::USIZE;
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
        slots: WCNodeSlots,
        paging_untyped: LocalCap<WUntyped>,
        // Things relating to user image code
        code_image_config: ProcessCodeImageConfig,
        user_image: &UserImage<role::Local>,
        parent_cnode: &LocalCap<LocalCNode>,
    ) -> Result<Self, VSpaceError> {
        let (code_slots, slots) = match slots.split(user_image.pages_count()) {
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
                    let _ = vspace.map_given_page(copied_page_cap, CapRights::R)?;
                }
            }
            ProcessCodeImageConfig::ReadWritable {
                parent_vspace_scratch,
                code_pages_ut,
                code_pages_slots,
            } => {
                // First, retype the untyped into `CodePageCount`
                // pages.
                // TODO - consider whether we should slice the UT/Slots resources needed here
                // off of the weak runtime resources made available to this VSpace
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
                    // TODO - do we need to manually pipe through a vaddr,
                    // or can we trust that setting the starting next_addr above
                    // combined with the user_image pages iterator will produce
                    // the right ordering of values?
                    let _mapped_page =
                        vspace.map_given_page(unmapped_region.to_page(), CapRights::RW)?;
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
            _state: PhantomData,
        })
    }

    /// `bootstrap` is used to wrap the root thread's address space.
    pub(crate) fn bootstrap(
        root_vspace_cptr: usize,
        next_addr: usize,
        cslots: WCNodeSlots,
        asid: LocalCap<AssignedASID>,
        ut: LocalCap<WUntyped>,
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
            next_addr,
            asid,
            _state: PhantomData,
        }
    }

    /// Map a region of memory at some address, I don't care where.
    pub fn map_region<SizeBits: Unsigned>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        rights: CapRights,
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Exclusive>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.map_region_internal(region, rights)
    }

    /// Map a region of memory at some address, then move it to a
    /// different cspace.
    pub fn map_region_and_move<SizeBits: Unsigned, Role: CNodeRole>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        rights: CapRights,
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
            self.map_region_internal(region, rights)?;
        let vaddr = mapped_region.vaddr;
        let dest_init_cptr = dest_slots.cap_data.offset;

        for (page, slot) in mapped_region.caps.iter().zip(dest_slots.iter()) {
            let _ = page.move_to_slot(src_cnode, slot)?;
        }

        Ok(MappedMemoryRegion {
            caps: MappedPageRange::new(dest_init_cptr, vaddr, self.asid.cap_data.asid),
            asid: self.asid.cap_data.asid,
            _shared_status: PhantomData,
            _size_bits: PhantomData,
            vaddr,
        })
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
        let unmapped_sr: UnmappedMemoryRegion<_, shared_status::Shared> = UnmappedMemoryRegion {
            caps: region.caps.copy(cnode, slots, rights)?,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        };
        self.map_region_internal(unmapped_sr, rights)
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
    ) -> Result<MappedMemoryRegion<SizeBits, shared_status::Shared>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        self.map_region_internal(region, rights)
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
        Ok(UnmappedMemoryRegion {
            caps: CapRange::new(start_cptr),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        })
    }

    pub(crate) fn root_cptr(&self) -> usize {
        self.root.cptr
    }

    fn unmap_page(
        &mut self,
        page: LocalCap<Page<page_state::Mapped>>,
    ) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(page.cptr) } {
            0 => Ok(Cap {
                cptr: page.cptr,
                cap_data: Page {
                    state: page_state::Unmapped {},
                },
                _role: PhantomData,
            }),
            e => Err(SeL4Error::PageUnmap(e)),
        }
    }

    fn map_region_internal<SizeBits: Unsigned, SSIn: SharedStatus, SSOut: SharedStatus>(
        &mut self,
        region: UnmappedMemoryRegion<SizeBits, SSIn>,
        rights: CapRights,
    ) -> Result<MappedMemoryRegion<SizeBits, SSOut>, VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        let vaddr = self.next_addr;
        // create the mapped region first because we need to pluck out
        // the `start_cptr` before the iteration below consumes the
        // unmapped region.
        let mapped_region = MappedMemoryRegion {
            caps: MappedPageRange::new(region.caps.start_cptr, vaddr, self.asid()),
            asid: self.asid(),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
            vaddr,
        };
        for page_cap in region.caps.iter() {
            self.map_given_page(page_cap, rights)?;
        }
        Ok(mapped_region)
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
pub struct ReservedRegion<PageCount: Unsigned = crate::userland::process::StackPageCount> {
    vaddr: usize,
    asid: u32,
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
            let mapped_page = vspace.map_given_page(unmapped_page, CapRights::RW)?;
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

/// Borrow of a reserved region and its associated VSpace in order to support temporary mapping
pub struct ScratchRegion<'a, 'b, PageCount: Unsigned = crate::userland::process::StackPageCount> {
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
        if region.asid == vspace.asid.cap_data.asid {
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
                cap_data: WCNodeSlotsData { offset: 0, size: 0 },
            }
        }
        let unmapped_region_copy: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive> =
            UnmappedMemoryRegion {
                caps: CapRange::new(region.caps.start_cptr),
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            };
        let mut next_addr = start_vaddr;
        for page in unmapped_region_copy.caps.iter() {
            match self.vspace.layers.map_layer(
                &page,
                next_addr,
                &mut self.vspace.root,
                CapRights::RW,
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
        let mut mapped_region = MappedMemoryRegion {
            caps: MappedPageRange::new(
                region.caps.start_cptr,
                start_vaddr,
                self.reserved_region.asid,
            ),
            asid: self.reserved_region.asid,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
            vaddr: start_vaddr,
        };
        let res = f(&mut mapped_region);
        let _ = self.vspace.unmap_region(mapped_region)?;
        Ok(res)
    }
}

mod private {
    use super::shared_status::{Exclusive, Shared};
    pub trait SealedSharedStatus {}
    impl SealedSharedStatus for Shared {}
    impl SealedSharedStatus for Exclusive {}

    use super::vspace_state::{Empty, Imaged};
    pub trait SealedVSpaceState {}
    impl SealedVSpaceState for Empty {}
    impl SealedVSpaceState for Imaged {}
}
