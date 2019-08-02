use core::mem;

use selfe_sys::*;

use crate::cap::{AssignedASID, LocalCap, UnassignedASID};
use selfe_wrap::error::{APIError, APIMethod, ErrorExt};

impl LocalCap<UnassignedASID> {
    pub fn assign(
        self,
        global_dir: &mut LocalCap<crate::arch::PagingRoot>,
    ) -> Result<LocalCap<AssignedASID>, APIError> {
        unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, global_dir.cptr) }
            .as_result()
            .map_err(|e| APIError::new(APIMethod::ASIDPoolAssign, e))?;

        Ok(unsafe { mem::transmute(self) })
    }
}
