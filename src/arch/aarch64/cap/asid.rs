use core::marker::PhantomData;
use core::mem;

use selfe_sys::*;

use typenum::*;

use crate::arch::cap::*;
use crate::cap::{CapType, LocalCap};
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
        global_dir: LocalCap<PageGlobalDirectory>,
    ) -> Result<LocalCap<AssignedASID<U0>>, SeL4Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, global_dir.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        Ok(unsafe { mem::transmute(self) })
    }
}
