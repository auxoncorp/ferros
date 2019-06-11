use core::mem;

use selfe_sys::*;

use crate::arch::cap::*;
use crate::cap::{CapType, LocalCap};
use crate::error::SeL4Error;

#[derive(Debug)]
pub struct UnassignedASID {
    pub(crate) asid: usize,
}

impl CapType for UnassignedASID {}

#[derive(Debug)]
pub struct AssignedASID {
    pub(crate) asid: u32,
}

impl CapType for AssignedASID {}

#[derive(Debug)]
pub struct ThreadID {
    id: u32,
}

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        page_dir: &mut LocalCap<PageDirectory>,
    ) -> Result<LocalCap<AssignedASID>, SeL4Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, page_dir.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        Ok(unsafe { mem::transmute(self) })
    }
}
