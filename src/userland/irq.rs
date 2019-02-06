use sel4_sys::*;

use crate::userland::{role, Cap, IRQHandle, SeL4Error};

impl Cap<IRQHandle, role::Local> {
    pub fn ack(&self) -> Result<(), SeL4Error> {
        let err = unsafe { seL4_IRQHandler_Ack(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::IRQHandlerAck(err));
        }
        Ok(())
    }
}
