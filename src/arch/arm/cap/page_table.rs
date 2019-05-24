use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{CNodeRole, CapType, CopyAliasable, DirectRetype, Movable, PhantomCap};

#[derive(Debug)]
pub struct UnmappedPageTable {}

impl CapType for UnmappedPageTable {}

impl PhantomCap for UnmappedPageTable {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for UnmappedPageTable {
    type CopyOutput = Self;
}

impl DirectRetype for UnmappedPageTable {
    type SizeBits = U10;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

#[derive(Debug)]
pub struct MappedPageTable<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(crate) vaddr: usize,
    pub(crate) next_free_slot: usize,
    pub(crate) _free_slots: PhantomData<FreeSlots>,
    pub(crate) _role: PhantomData<Role>,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CapType for MappedPageTable<FreeSlots, Role> {}

impl<FreeSlots: Unsigned, Role: CNodeRole> CopyAliasable for MappedPageTable<FreeSlots, Role> {
    type CopyOutput = UnmappedPageTable;
}

impl<FreeSlots: Unsigned, Role: CNodeRole> Movable for MappedPageTable<FreeSlots, Role> {}
