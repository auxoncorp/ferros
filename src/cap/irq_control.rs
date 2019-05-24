use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{irq_state, CNodeRole, CNodeSlot, Cap, CapType, IRQHandler, LocalCap};
use crate::error::SeL4Error;

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
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        if self.cap_data.known_handled[IRQ::USIZE] {
            return Err(IRQError::UnavailableIRQ);
        }
        let err = unsafe {
            seL4_IRQControl_Get(
                self.cptr,           // service/authority
                IRQ::I32,            // irq
                dest_cptr,           // root
                dest_offset,         // index
                seL4_WordBits as u8, // depth
            )
        };
        if err != 0 {
            return Err(IRQError::SeL4Error(SeL4Error::IRQControlGet(err)));
        }

        self.cap_data.known_handled[IRQ::USIZE] = true;

        Ok(Cap {
            cptr: dest_offset,
            cap_data: IRQHandler {
                _irq: PhantomData,
                _set_state: PhantomData,
            },
            _role: PhantomData,
        })
    }
}
