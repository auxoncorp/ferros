use core::marker::PhantomData;

use crate::cap::{role, Cap, CapType, DirectRetype, WCNodeSlots, WUntyped};
use crate::userland::CapRights;

/// A `Maps` implementor is a paging layer that maps granules of type
/// `G`. The if this layer isn't present for the incoming address,
/// `MappingError::Overflow` should be returned, as this signals to
/// the caller—the layer above—that it needs to create a new object at
/// this layer and then attempt again to map the `item`.
pub trait Maps<G> {
    fn map_item<RootG, Root>(
        &mut self,
        item: &G,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
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
    type Item: DirectRetype;
    fn map_item<RootG, Root>(
        &mut self,
        item: &Self::Item,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
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
{
    pub layer: Cap<L, role::Local>,
    _item: PhantomData<G>,
}

impl<G, L: Maps<G>> PagingLayer for PagingTop<G, L>
where
    L: CapType,
    G: DirectRetype,
{
    type Item = G;
    fn map_item<RootG, Root>(
        &mut self,
        item: &G,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
        rights: CapRights,
        ut: &mut WUntyped,
        slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        self.layer
            .cap_data
            .map_item(item, addr, root, rights, ut, slots)
    }
}

pub struct PagingRec<G, L: Maps<G>, P: PagingLayer> {
    layer: L,
    next: P,
    _item: PhantomData<G>,
}

impl<G, L: Maps<G>, P: PagingLayer> PagingLayer for PagingRec<G, L, P>
where
    L: CapType,
    G: DirectRetype,
{
    type Item = G;
    fn map_item<RootG, Root>(
        &mut self,
        item: &G,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
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

struct VSpace<L: PagingLayer> {
    layers: L,
    next_page: usize,
    untyped: WUntyped,
    slots: WCNodeSlots,
}

impl<L: PagingLayer> VSpace<L> {}
