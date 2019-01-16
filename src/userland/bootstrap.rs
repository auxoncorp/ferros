use core::marker::PhantomData;
use core::mem::size_of;
use core::ops::Sub;
use crate::userland::{
    role, ASIDControl, ASIDPool, AssignedPageDirectory, CNode, Cap, Error, UnassignedPageDirectory,
    Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1, U12};

// The ASID pool needs an untyped of exactly 4k
impl Cap<Untyped<U12>, role::Local> {
    // TODO put retype local into a trait so we can dispatch via the target cap type
    pub fn retype_asid_pool<FreeSlots: Unsigned>(
        self,
        asid_control: Cap<ASIDControl, role::Local>,
        dest_cnode: CNode<FreeSlots, role::Local>,
    ) -> Result<
        (
            Cap<ASIDPool, role::Local>,
            CNode<Sub1<FreeSlots>, role::Local>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                asid_control.cptr,              // _service
                self.cptr,                      // untyped
                dest_slot.cptr,                 // root
                dest_slot.offset,               // index
                (8 * size_of::<usize>()) as u8, // depth
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }
}

impl Cap<ASIDPool, role::Local> {
    pub fn assign(
        &mut self,
        vspace: Cap<UnassignedPageDirectory, role::Local>,
    ) -> Result<Cap<AssignedPageDirectory, role::Local>, Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, vspace.cptr) };

        if err != 0 {
            return Err(Error::ASIDPoolAssign(err));
        }

        Ok(Cap {
            cptr: vspace.cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
        })
    }
}
