use core::marker::PhantomData;

use typenum::*;

use crate::arch::cap::Page;
use crate::arch::{AddressSpace, PageBits, PagingRoot};
use crate::cap::{CapType, DirectRetype, LocalCap, WCNodeSlots, WUntyped};
use crate::error::SeL4Error;
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
    PageMapFailure(u32),
    IntermediateLayerFailure(u32),
    RetypingError,
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

impl VSpace {
    pub fn new(mut untyped: WUntyped, mut slots: WCNodeSlots) -> Result<Self, SeL4Error> {
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
        page: &LocalCap<Page>,
        rights: CapRights,
    ) -> Result<(), SeL4Error> {
        self.layers
            .map_item(
                page,
                self.next_addr,
                &mut self.root,
                rights,
                &mut self.untyped,
                &mut self.slots,
            )
            .map_err(|e| SeL4Error::MapPage(e))?;
        self.next_addr += PageBits::USIZE;
        Ok(())
    }

    pub fn map_page(&mut self, rights: CapRights) -> Result<LocalCap<Page>, SeL4Error> {
        let page = self.untyped.retype::<Page>(&mut self.slots)?;
        self.map_given_page(&page, rights)?;
        Ok(page)
    }
}
