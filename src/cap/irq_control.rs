use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{
    irq_handler, irq_state, CNodeRole, CNodeSlot, Cap, CapType, IRQHandler, LocalCap,
};
use crate::error::{ErrorExt, SeL4Error};

// TODO - consider moving IRQ code allocation tracking to compile-time,
// which may be feasible since we treat IRQControl as a global
// singleton.
// The goal of such tracking is to prevent accidental double-binding to a single IRQ
pub struct IRQControl {
    pub(crate) known_handled: [bool; 256],
}

impl CapType for IRQControl {}

#[derive(Debug)]
pub enum IRQError {
    UnavailableIRQ,
    SeL4Error(SeL4Error),
}
impl From<SeL4Error> for IRQError {
    fn from(e: SeL4Error) -> Self {
        IRQError::SeL4Error(e)
    }
}

impl LocalCap<IRQControl> {
    pub fn create_handler<IRQ: Unsigned, DestRole: CNodeRole>(
        &mut self,
        dest_slot: CNodeSlot<DestRole>,
    ) -> Result<Cap<IRQHandler<IRQ, irq_state::Unset>, DestRole>, IRQError>
    where
        IRQ: IsLess<U256, Output = True>,
    {
        let destination_relative_cptr = self.internal_create_handler(dest_slot, IRQ::U8)?;
        Ok(Cap {
            cptr: destination_relative_cptr,
            cap_data: IRQHandler {
                _irq: PhantomData,
                _set_state: PhantomData,
            },
            _role: PhantomData,
        })
    }

    pub fn create_weak_handler<DestRole: CNodeRole>(
        &mut self,
        dest_slot: CNodeSlot<DestRole>,
        irq: u8,
    ) -> Result<Cap<irq_handler::weak::WIRQHandler<irq_state::Unset>, DestRole>, IRQError> {
        let destination_relative_cptr = self.internal_create_handler(dest_slot, irq)?;
        Ok(Cap {
            cptr: destination_relative_cptr,
            cap_data: irq_handler::weak::WIRQHandler {
                irq,
                _set_state: PhantomData,
            },
            _role: PhantomData,
        })
    }

    fn internal_create_handler<DestRole: CNodeRole>(
        &mut self,
        dest_slot: CNodeSlot<DestRole>,
        irq: u8,
    ) -> Result<usize, IRQError> {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        if self.cap_data.known_handled[usize::from(irq)] {
            return Err(IRQError::UnavailableIRQ);
        }
        unsafe {
            seL4_IRQControl_Get(
                self.cptr,           // service/authority
                usize::from(irq),    // irq
                dest_cptr,           // root
                dest_offset,         // index
                seL4_WordBits as u8, // depth
            )
        }
        .as_result()
        .map_err(|e| IRQError::SeL4Error(SeL4Error::IRQControlGet(e)))?;

        self.cap_data.known_handled[usize::from(irq)] = true;
        Ok(dest_offset)
    }
}
