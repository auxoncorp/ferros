use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{Cap, CapType, LocalCap, Movable, Notification};
use crate::error::{ErrorExt, SeL4Error};

/// Whether or not an IRQ Handle has been set to a particular Notification
pub trait IRQSetState: private::SealedIRQSetState {}

pub mod irq_state {
    use super::IRQSetState;

    /// Not set to a Notification
    #[derive(Debug, PartialEq)]
    pub struct Unset;
    impl IRQSetState for Unset {}

    /// Set to a Notification
    #[derive(Debug, PartialEq)]
    pub struct Set;
    impl IRQSetState for Set {}
}

pub struct IRQHandler<IRQ: Unsigned, SetState: IRQSetState>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub(crate) _irq: PhantomData<IRQ>,
    pub(crate) _set_state: PhantomData<SetState>,
}

impl<IRQ: Unsigned, SetState: IRQSetState> CapType for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<U256, Output = True>
{
}

impl<IRQ: Unsigned, SetState: IRQSetState> Movable for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<U256, Output = True>
{
}

impl<IRQ: Unsigned> LocalCap<IRQHandler<IRQ, irq_state::Unset>>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub(crate) fn set_notification(
        self,
        notification: &LocalCap<Notification>,
    ) -> Result<(LocalCap<IRQHandler<IRQ, irq_state::Set>>), SeL4Error> {
        unsafe { seL4_IRQHandler_SetNotification(self.cptr, notification.cptr) }
            .as_result()
            .map_err(|e| SeL4Error::IRQHandlerSetNotification(e))?;
        Ok(Cap {
            cptr: self.cptr,
            _role: self._role,
            cap_data: IRQHandler {
                _irq: self.cap_data._irq,
                _set_state: PhantomData,
            },
        })
    }
}

impl<IRQ: Unsigned> LocalCap<IRQHandler<IRQ, irq_state::Set>>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub fn ack(&self) -> Result<(), SeL4Error> {
        unsafe { seL4_IRQHandler_Ack(self.cptr) }
            .as_result()
            .map_err(|e| SeL4Error::IRQHandlerAck(e))
    }
}

mod private {
    pub trait SealedIRQSetState {}
    impl SealedIRQSetState for super::irq_state::Unset {}
    impl SealedIRQSetState for super::irq_state::Set {}
}
