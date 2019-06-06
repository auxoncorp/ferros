use core::marker::PhantomData;
use core::ops::{Add, Div, Mul};

use typenum::*;

use selfe_sys::*;

use crate::arch::cap::{page_state, Page};
use crate::arch::{AddressSpace, PageBits, PageBytes, PagingRoot};
use crate::cap::{
    CNodeRole, Cap, CapRange, CapType, DirectRetype, KernelRetypeFanOutLimit, LocalCNodeSlots,
    LocalCap, PhantomCap, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped,
};
use crate::error::SeL4Error;
use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;

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

pub enum MappingError {
    Overflow,
    AddrNotPageAligned,
    PageMapFailure(SeL4Error),
    IntermediateLayerFailure(SeL4Error),
    RetypingError,
}

pub enum VSpaceError {
    TooBig,
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
    _item: PhantomData<G>,
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
    layer: L,
    next: P,
    _item: PhantomData<G>,
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

pub struct VSpace {
    layers: AddressSpace,
    root: LocalCap<PagingRoot>,
    next_addr: usize,
    untyped: WUntyped,
    slots: WCNodeSlots,
}

// numPages x = ((2 ^ x) / pageBytes) + 1
type NumPages<Size> = Sum<Quot<Pow<Size>, U4096>, U1>;

struct UnmappedMemoryRegion<Role: CNodeRole, SizeBits: Unsigned>
where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: _Pow,
    <SizeBits as _Pow>::Output: Div<U4096>,
    <<SizeBits as _Pow>::Output as Div<U4096>>::Output: Add<U1>,
    <<<SizeBits as _Pow>::Output as Div<U4096>>::Output as Add<U1>>::Output: Unsigned,
{
    caps: CapRange<Page<page_state::Unmapped>, Role, NumPages<SizeBits>>,
    _size_bits: PhantomData<SizeBits>,
}

impl<Role: CNodeRole, SizeBits: Unsigned> UnmappedMemoryRegion<Role, SizeBits>
where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: _Pow,
    <SizeBits as _Pow>::Output: Div<U4096>,
    <<SizeBits as _Pow>::Output as Div<U4096>>::Output: Add<U1>,
    <<<SizeBits as _Pow>::Output as Div<U4096>>::Output as Add<U1>>::Output: Unsigned,
    NumPages<SizeBits>: Unsigned,
    NumPages<SizeBits>: Mul<<Page<page_state::Unmapped> as DirectRetype>::SizeBits>,
{
    pub(crate) fn new(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
    ) -> Result<Self, VSpaceError> {
        let page_caps = ut.retype_multi::<Page<page_state::Unmapped>, NumPages<SizeBits>>(slots)?;
        Ok(UnmappedMemoryRegion {
            caps: CapRange {
                start_cptr: page_caps.start_cptr,
                _cap_type: PhantomData,
                _role: PhantomData,
                _slots: PhantomData,
            },
            _size_bits: PhantomData,
        })
    }
}

struct PageRange<Role: CNodeRole, Count: Unsigned> {
    initial_cptr: usize,
    initial_vaddr: usize,
    asid: usize,
    _count: PhantomData<Count>,
    _role: PhantomData<Role>,
}

impl<Role: CNodeRole, Count: Unsigned> PageRange<Role, Count> {
    fn new(initial_cptr: usize, initial_vaddr: usize, asid: usize) -> Self {
        PageRange {
            initial_cptr,
            initial_vaddr,
            asid,
            _count: PhantomData,
            _role: PhantomData,
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Cap<Page<page_state::Mapped>, Role>> {
        (0..Count::USIZE).map(|idx| Cap {
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

struct MappedMemoryRegion<Role: CNodeRole, SizeBits: Unsigned>
where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: _Pow,
    <SizeBits as _Pow>::Output: Div<U4096>,
    <<SizeBits as _Pow>::Output as Div<U4096>>::Output: Add<U1>,
    <<<SizeBits as _Pow>::Output as Div<U4096>>::Output as Add<U1>>::Output: Unsigned,
{
    caps: PageRange<Role, NumPages<SizeBits>>,
    asid: usize,
    vaddr: usize,
    _size_bits: PhantomData<SizeBits>,
}

impl<Role: CNodeRole, SizeBits: Unsigned> MappedMemoryRegion<Role, SizeBits>
where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: _Pow,
    <SizeBits as _Pow>::Output: Div<U4096>,
    <<SizeBits as _Pow>::Output as Div<U4096>>::Output: Add<U1>,
    <<<SizeBits as _Pow>::Output as Div<U4096>>::Output as Add<U1>>::Output: Unsigned,
{
    pub(crate) fn new(
        region: UnmappedMemoryRegion<Role, SizeBits>,
        vaddr: usize,
        asid: usize,
    ) -> Self {
        MappedMemoryRegion {
            caps: PageRange::new(region.caps.start_cptr, vaddr, asid),
            vaddr,
            asid,
        }
    }
}

impl VSpace {
    pub(crate) fn bootstrap(
        root_vspace_cptr: usize,
        next_addr: usize,
        root_cnode_cptr: usize,
    ) -> Self {
        VSpace {
            layers: AddressSpace::new(),
            root: Cap {
                cptr: root_vspace_cptr,
                cap_data: PagingRoot::phantom_instance(),
                _role: PhantomData,
            },
            next_addr,
            untyped: WUntyped { size: 0 },
            slots: Cap {
                cptr: root_cnode_cptr,
                cap_data: WCNodeSlotsData { offset: 0, size: 0 },
                _role: PhantomData,
            },
        }
    }

    pub fn new(untyped: WUntyped, mut slots: WCNodeSlots) -> Result<Self, SeL4Error> {
        let root = untyped.retype::<PagingRoot>(&mut slots)?;
        Ok(VSpace {
            layers: AddressSpace::new(),
            next_addr: 0,
            untyped,
            slots,
            root,
        })
    }

    pub fn map_given_page(
        &mut self,
        page: LocalCap<Page<page_state::Unmapped>>,
        rights: CapRights,
    ) -> Result<LocalCap<Page<page_state::Mapped>>, SeL4Error> {
        match self.layers.map_item(
            &page,
            self.next_addr,
            &mut self.root,
            rights,
            &mut self.untyped,
            &mut self.slots,
        ) {
            Err(MappingError::PageMapFailure(e)) => return Err(e),
            Err(MappingError::IntermediateLayerFailure(e)) => {
                return Err(e);
            }
            Ok(_) => (),
        };
        let vaddr = self.next_addr;
        self.next_addr += PageBits::USIZE;
        Ok(Cap {
            cptr: page.cptr,
            cap_data: Page {
                state: page_state::Mapped { vaddr },
            },
            _role: PhantomData,
        })
    }

    pub fn map_page(
        &mut self,
        rights: CapRights,
    ) -> Result<LocalCap<Page<page_state::Mapped>>, SeL4Error> {
        let page = self
            .untyped
            .retype::<Page<page_state::Unmapped>>(&mut self.slots)?;
        self.map_given_page(page, rights)
    }

    pub fn temporarily_map_page<F>(
        &mut self,
        page: LocalCap<Page<page_state::Unmapped>>,
        f: F,
    ) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error>
    where
        F: Fn(&mut LocalCap<Page<page_state::Mapped>>) -> Result<(), SeL4Error>,
    {
        let mut mapped_page = self.map_given_page(page, CapRights::RW)?;
        let res = f(&mut mapped_page);
        self.unmap_page(mapped_page);
        res.map(|_| Cap {
            cptr: mapped_page.cptr,
            cap_data: Page {
                state: page_state::Unmapped {},
            },
            _role: PhantomData,
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
}
