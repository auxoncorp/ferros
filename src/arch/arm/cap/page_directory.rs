use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{CNodeRole, CapType, DirectRetype, PhantomCap};

// TODO: It's important that AssignedPageDirectory can never be moved or deleted
// (or copied, likely), as that leads to ugly cptr aliasing issues that we're
// not able to detect at compile time. Write compile-tests to ensure that it
// doesn't implement those marker traits.
#[derive(Debug, PartialEq)]
pub struct AssignedPageDirectory<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(crate) next_free_slot: usize,
    pub(crate) _free_slots: PhantomData<FreeSlots>,
    pub(crate) _role: PhantomData<Role>,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CapType for AssignedPageDirectory<FreeSlots, Role> {}

#[derive(Debug)]
pub struct UnassignedPageDirectory {}

impl CapType for UnassignedPageDirectory {}

impl PhantomCap for UnassignedPageDirectory {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for UnassignedPageDirectory {
    type SizeBits = U14;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}
