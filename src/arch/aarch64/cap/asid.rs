use core::mem;

use selfe_sys::*;

use crate::arch::cap::*;
use crate::cap::{CapType, InternalASID, LocalCap};
use crate::error::{ErrorExt, SeL4Error};

#[derive(Debug)]
pub struct UnassignedASID {
    pub(crate) asid: InternalASID,
}

impl CapType for UnassignedASID {}

#[derive(Debug)]
pub struct AssignedASID {
    pub(crate) asid: InternalASID,
}

impl CapType for AssignedASID {}

#[derive(Debug)]
pub struct ThreadID {
    id: u32,
}

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        global_dir: &mut LocalCap<PageGlobalDirectory>,
    ) -> Result<LocalCap<AssignedASID>, SeL4Error> {
        unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, global_dir.cptr) }
            .as_result()
            .map_err(|e| SeL4Error::ASIDPoolAssign(e))?;

        Ok(unsafe { mem::transmute(self) })
    }
}
