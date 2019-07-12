use crate::cap::irq_handler::irq_state;
use crate::cap::irq_handler::weak::{self, WIRQHandler};
use crate::cap::{CNodeRole, Cap, IRQControl, IRQError, LocalCap, MaxIRQCount, WCNodeSlotsData};
use crate::userland::*;
use arrayvec::ArrayVec;
use typenum::*;

#[derive(Debug)]
pub enum IRQCollectionError {
    IRQError(IRQError),
    NotEnoughSlots,
}

pub struct WIRQHandlerCollection<Role: CNodeRole> {
    handlers: ArrayVec<[Cap<WIRQHandler<irq_state::Unset>, Role>; MaxIRQCount::USIZE]>,
}

impl<Role: CNodeRole> WIRQHandlerCollection<Role> {
    pub fn new(
        irq_control: &mut LocalCap<IRQControl>,
        dest_slots: &mut LocalCap<WCNodeSlotsData<Role>>,
        requested_irqs: [bool; MaxIRQCount::USIZE],
    ) -> Result<Self, IRQCollectionError> {
        let requested_count = requested_irqs.iter().filter(|r| **r).count();
        if dest_slots.cap_data.size < requested_count {
            return Err(IRQCollectionError::NotEnoughSlots);
        }
        // First pass to detect requested-but-unavailable IRQs and reject the request without mutation
        for (irq, (is_requested, is_available)) in (0..MaxIRQCount::U16).zip(
            requested_irqs
                .iter()
                .zip(irq_control.cap_data.available.iter()),
        ) {
            if *is_requested && !*is_available {
                return Err(IRQCollectionError::IRQError(IRQError::UnavailableIRQ(irq)));
            }
        }
        let mut handlers = ArrayVec::new();
        for (irq, is_requested) in (0..MaxIRQCount::U16).zip(requested_irqs.iter()) {
            // The source instance of IRQControl should treat the IRQs split off into the other
            // instance as if they were unavailable.
            if *is_requested {
                let slot = dest_slots
                    .alloc_single()
                    .map_err(|_| IRQCollectionError::NotEnoughSlots)?;
                handlers.push(
                    irq_control
                        .create_weak_handler(slot, irq)
                        .map_err(|e| IRQCollectionError::IRQError(e))?,
                );
            }
        }
        Ok(WIRQHandlerCollection { handlers })
    }

    pub fn get_weak_handler(
        &mut self,
        irq: u16,
    ) -> Option<Cap<weak::WIRQHandler<irq_state::Unset>, Role>> {
        if let Some(index) =
            self.handlers
                .iter()
                .enumerate()
                .find_map(|(i, h)| if h.irq() == irq { Some(i) } else { None })
        {
            Some(self.handlers.remove(index))
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }
}
