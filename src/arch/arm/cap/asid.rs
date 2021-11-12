use core::mem;

use selfe_sys::*;

use crate::cap::{AssignedASID, LocalCap, UnassignedASID};
use crate::error::{ErrorExt, SeL4Error};

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        global_dir: &mut LocalCap<crate::arch::PagingRoot>,
    ) -> Result<LocalCap<AssignedASID>, SeL4Error> {
        unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, global_dir.cptr) }
            .as_result()
            .map_err(SeL4Error::ASIDPoolAssign)?;

        Ok(unsafe { mem::transmute(self) })
    }
}
