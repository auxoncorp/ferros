use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{
    irq_handler, irq_state, CNodeRole, CNodeSlot, Cap, CapType, IRQHandler, LocalCNode, LocalCap,
};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

pub type MaxIRQCount = U1024;

// The goal of tracking is to prevent accidental double-binding to a single IRQ
pub struct IRQControl {
    /// Is the IRQ whose id matches the index available to be claimed/create-a-handler for it?
    ///
    /// If the value at a given index is false, one of the following is the case:
    /// * An IRQHandler has been created for that IRQ
    /// * A different IRQControl instance is responsible for managing that IRQ
    /// * The bootstrapping code has decided to reserve that IRQ for some non-user-facing purpose
    pub(crate) available: [bool; MaxIRQCount::USIZE],
}

impl CapType for IRQControl {}

#[derive(Debug)]
pub enum IRQError {
    /// The IRQ has already been claimed
    UnavailableIRQ(u16),
    /// The IRQ requested is not in the supported range of possible IRQs
    OutOfRangeIRQ(u16),
    /// The kernel has a problem with how IRQ management is proceeding
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
        IRQ: IsLess<U1024, Output = True>,
    {
        let destination_relative_cptr = self.internal_create_handler(dest_slot, IRQ::U16)?;
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
        irq: u16,
    ) -> Result<Cap<irq_handler::weak::WIRQHandler<irq_state::Unset>, DestRole>, IRQError> {
        if irq >= MaxIRQCount::U16 {
            return Err(IRQError::OutOfRangeIRQ(irq));
        }
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
        irq: u16,
    ) -> Result<usize, IRQError> {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        if !self.cap_data.available[usize::from(irq)] {
            return Err(IRQError::UnavailableIRQ(irq));
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

        self.cap_data.available[usize::from(irq)] = false;
        Ok(dest_offset)
    }

    pub fn request_split<DestRole: CNodeRole>(
        &mut self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
        requested_irqs: [bool; MaxIRQCount::USIZE],
    ) -> Result<Cap<IRQControl, DestRole>, IRQError> {
        // First pass to detect requested-but-unavailable IRQs and reject the request without mutation
        for (irq, (is_requested, is_available)) in
            (0..MaxIRQCount::U16).zip(requested_irqs.iter().zip(self.cap_data.available.iter()))
        {
            if *is_requested && !*is_available {
                return Err(IRQError::UnavailableIRQ(irq));
            }
        }

        let dest_offset = self.unchecked_copy(src_cnode, dest_slot, CapRights::RWG)?;

        let mut split_side_available_irqs = requested_irqs;
        for (claimed_for_split_side, source_side_available_state) in split_side_available_irqs
            .iter()
            .zip(self.cap_data.available.iter_mut())
        {
            // The source instance of IRQControl should treat the IRQs split off into the other
            // instance as if they were unavailable.
            if *claimed_for_split_side {
                *source_side_available_state = false;
            }
        }
        Ok(Cap {
            cptr: dest_offset,
            cap_data: IRQControl {
                available: split_side_available_irqs,
            },
            _role: PhantomData,
        })
    }
}
