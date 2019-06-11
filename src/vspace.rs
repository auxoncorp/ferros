use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use selfe_sys::*;

use crate::arch::cap::{page_state, AssignedASID, Page, UnassignedASID};
use crate::arch::{AddressSpace, PageBits, PageBytes, PagingRoot};
use crate::cap::{
    role, Cap, CapRange, CapType, DirectRetype, LocalCNode, LocalCNodeSlots, LocalCap, PhantomCap,
    RetypeError, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped,
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

/// A `Maps` implementor is a paging layer that maps granules of type
/// `G`. The if this layer isn't present for the incoming address,
/// `MappingError::Overflow` should be returned, as this signals to
/// the caller—the layer above—that it needs to create a new object at
/// this layer and then attempt again to map the `item`.
pub trait Maps<G: CapType> {
    fn map_item<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        ut: &mut WUntyped,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType;
}

#[derive(Debug)]
pub enum MappingError {
    Overflow,
    AddrNotPageAligned,
    PageMapFailure(SeL4Error),
    IntermediateLayerFailure(SeL4Error),
    RetypingError,
}

#[derive(Debug)]
pub enum VSpaceError {
    TooBig,
    MappingError(MappingError),
    RetypeRegion(RetypeError),
    SeL4Error(SeL4Error),
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

pub trait PagingLayer {
    type Item: DirectRetype + CapType;
    fn map_item<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<Self::Item>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        ut: &mut WUntyped,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType;
}

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
    G: DirectRetype,
    G: CapType,
{
    type Item = G;
    fn map_item<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        ut: &mut WUntyped,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        self.layer.map_item(item, addr, root, rights, ut, slots)
    }
}

pub struct PagingRec<G: CapType, L: Maps<G>, P: PagingLayer> {
    pub(crate) layer: L,
    pub(crate) next: P,
    pub(crate) _item: PhantomData<G>,
}

impl<G, L: Maps<G>, P: PagingLayer> PagingLayer for PagingRec<G, L, P>
where
    L: CapType,
    G: DirectRetype,
    G: CapType,
{
    type Item = G;
    fn map_item<RootG: CapType, Root>(
        &mut self,
        item: &LocalCap<G>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
        ut: &mut WUntyped,
        mut slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        match self.layer.map_item(item, addr, root, rights, ut, slots) {
            Err(MappingError::Overflow) => {
                let next_item = match ut.retype::<P::Item>(&mut slots) {
                    Ok(i) => i,
                    Err(_) => return Err(MappingError::RetypingError),
                };
                self.next
                    .map_item(&next_item, addr, root, rights, ut, slots)?;
                self.layer.map_item(item, addr, root, rights, ut, slots)
            }
            res => res,
        }
    }
}

type NumPages<Size> = Pow<op!(Size - PageBits)>;

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

impl<SizeBits: Unsigned> UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(crate) fn new(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
    ) -> Result<Self, VSpaceError> {
        let page_caps =
            ut.retype_multi_runtime::<Page<page_state::Unmapped>, NumPages<SizeBits>>(slots)?;
        Ok(UnmappedMemoryRegion {
            caps: CapRange::new(page_caps.start_cptr),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        })
    }

    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion {
            caps: CapRange::new(self.caps.start_cptr),
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
}

pub struct MappedMemoryRegion<SizeBits: Unsigned, SS: SharedStatus>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub vaddr: usize,
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
    pub(crate) fn new(
        region: UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        vaddr: usize,
        asid: u32,
    ) -> Self {
        MappedMemoryRegion {
            caps: MappedPageRange::new(region.caps.start_cptr, vaddr, asid),
            _size_bits: PhantomData,
            _shared_status: PhantomData,
            vaddr,
            asid,
        }
    }
}

pub struct VSpace {
    root: LocalCap<PagingRoot>,
    asid: LocalCap<AssignedASID>,
    layers: AddressSpace,
    next_addr: usize,
    untyped: WUntyped,
    slots: WCNodeSlots,
}

impl VSpace {
    pub fn new(
        mut root_cap: LocalCap<PagingRoot>,
        asid: LocalCap<UnassignedASID>,
        slots: WCNodeSlots,
        untyped: WUntyped,
    ) -> Result<Self, VSpaceError> {
        let assigned_asid = asid.assign(&mut root_cap)?;
        Ok(VSpace {
            root: root_cap,
            asid: assigned_asid,
            layers: AddressSpace::new(),
            next_addr: 0,
            untyped,
            slots,
        })
    }

    pub(crate) fn bootstrap(
        root_vspace_cptr: usize,
        next_addr: usize,
        root_cnode_cptr: usize,
        asid: LocalCap<AssignedASID>,
    ) -> Self {
        VSpace {
            layers: AddressSpace::new(),
            root: Cap {
                cptr: root_vspace_cptr,
                cap_data: PagingRoot::phantom_instance(),
                _role: PhantomData,
            },
            untyped: WUntyped { size: 0 },
            slots: Cap {
                cptr: root_cnode_cptr,
                cap_data: WCNodeSlotsData { offset: 0, size: 0 },
                _role: PhantomData,
            },
            next_addr,
            asid,
        }
    }

    pub fn asid(&self) -> u32 {
        self.asid.cap_data.asid
    }

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

    pub fn map_given_page(
        &mut self,
        page: LocalCap<Page<page_state::Unmapped>>,
        rights: CapRights,
    ) -> Result<LocalCap<Page<page_state::Mapped>>, VSpaceError> {
        match self.layers.map_item(
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
        self.next_addr += PageBits::USIZE;
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

    pub fn map_page(
        &mut self,
        rights: CapRights,
    ) -> Result<LocalCap<Page<page_state::Mapped>>, VSpaceError> {
        let page = self
            .untyped
            .retype::<Page<page_state::Unmapped>>(&mut self.slots)?;
        self.map_given_page(page, rights)
    }

    pub fn temporarily_map_region<SizeBits: Unsigned, F>(
        &mut self,
        region: &mut UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>,
        f: F,
    ) -> Result<(), VSpaceError>
    where
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
        F: Fn(&mut MappedMemoryRegion<SizeBits, shared_status::Exclusive>) -> Result<(), SeL4Error>,
    {
        let mut mapped_region = self.map_region(
            UnmappedMemoryRegion {
                caps: CapRange::new(region.caps.start_cptr),
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            CapRights::RW,
        )?;
        let res = f(&mut mapped_region);
        let _ = self.unmap_region(mapped_region)?;
        let _ = res?;
        Ok(())
    }

    pub fn unmap_region<SizeBits: Unsigned>(
        &mut self,
        region: MappedMemoryRegion<SizeBits, shared_status::Exclusive>,
    ) -> Result<UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>, VSpaceError>
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

    pub fn unmap_page(
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

    pub(crate) fn root_cptr(&self) -> usize {
        self.root.cptr
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
}

mod private {
    use super::shared_status::{Exclusive, Shared};
    pub trait SealedSharedStatus {}
    impl SealedSharedStatus for Shared {}
    impl SealedSharedStatus for Exclusive {}
}
