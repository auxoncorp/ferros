use core::marker::PhantomData;
use core::mem;

use selfe_sys::*;

use typenum::*;

use crate::arch::cap::*;
use crate::arch::*;
use crate::cap::{role, Cap, CapType, LocalCap};
use crate::error::SeL4Error;

#[derive(Debug)]
pub struct UnassignedASID {
    pub(crate) asid: usize,
}

impl CapType for UnassignedASID {}

#[derive(Debug)]
pub struct AssignedASID<ThreadCount: Unsigned> {
    asid: u32,
    _thread_count: PhantomData<ThreadCount>,
}

impl<ThreadCount: Unsigned> CapType for AssignedASID<ThreadCount> {}

#[derive(Debug)]
pub struct ThreadID {
    id: u32,
}

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<
        (
            LocalCap<AssignedASID<U0>>,
            LocalCap<AssignedPageDirectory<BasePageDirFreeSlots, role::Child>>,
        ),
        SeL4Error,
    > {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, page_dir.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        let page_dir = Cap {
            cptr: page_dir.cptr,
            _role: PhantomData,
            cap_data: AssignedPageDirectory {
                next_free_slot: 0,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        };

        Ok((unsafe { mem::transmute(self) }, page_dir))
    }
}
