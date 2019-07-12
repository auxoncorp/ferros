use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::irq_handler::weak::WIRQHandler;
use crate::cap::{Cap, CapType, LocalCap, MaxIRQCount, Movable, Notification, PhantomCap};
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
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub(crate) _irq: PhantomData<IRQ>,
    pub(crate) _set_state: PhantomData<SetState>,
}

impl<IRQ: Unsigned, SetState: IRQSetState> CapType for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<MaxIRQCount, Output = True>
{
}

impl<IRQ: Unsigned, SetState: IRQSetState> Movable for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<MaxIRQCount, Output = True>
{
}

impl<IRQ: Unsigned, SetState: IRQSetState> PhantomCap for IRQHandler<IRQ, SetState>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    fn phantom_instance() -> Self {
        Self {
            _irq: PhantomData,
            _set_state: PhantomData,
        }
    }
}

impl<IRQ: Unsigned, SetState: IRQSetState> LocalCap<IRQHandler<IRQ, SetState>>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn weaken(self) -> LocalCap<weak::WIRQHandler<SetState>> {
        Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: WIRQHandler {
                irq: IRQ::U16,
                _set_state: PhantomData,
            },
        }
    }
}

impl<IRQ: Unsigned> LocalCap<IRQHandler<IRQ, irq_state::Unset>>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn set_notification(
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
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn ack(&self) -> Result<(), SeL4Error> {
        unsafe { seL4_IRQHandler_Ack(self.cptr) }
            .as_result()
            .map_err(|e| SeL4Error::IRQHandlerAck(e))
    }
}

pub mod weak {
    use super::*;

    pub struct WIRQHandler<SetState: IRQSetState> {
        pub(crate) irq: u16,
        pub(crate) _set_state: PhantomData<SetState>,
    }

    impl<SetState: IRQSetState> CapType for WIRQHandler<SetState> {}

    impl<SetState: IRQSetState> Movable for WIRQHandler<SetState> {}

    impl<Role: crate::cap::CNodeRole, SetState: IRQSetState> Cap<WIRQHandler<SetState>, Role> {
        pub fn irq(&self) -> u16 {
            self.cap_data.irq
        }
    }
    impl LocalCap<WIRQHandler<irq_state::Unset>> {
        pub fn set_notification(
            self,
            notification: &LocalCap<Notification>,
        ) -> Result<(LocalCap<WIRQHandler<irq_state::Set>>), SeL4Error> {
            unsafe { seL4_IRQHandler_SetNotification(self.cptr, notification.cptr) }
                .as_result()
                .map_err(|e| SeL4Error::IRQHandlerSetNotification(e))?;
            Ok(Cap {
                cptr: self.cptr,
                _role: self._role,
                cap_data: WIRQHandler {
                    irq: self.cap_data.irq,
                    _set_state: PhantomData,
                },
            })
        }
    }

    impl LocalCap<WIRQHandler<irq_state::Set>> {
        pub fn ack(&self) -> Result<(), SeL4Error> {
            unsafe { seL4_IRQHandler_Ack(self.cptr) }
                .as_result()
                .map_err(|e| SeL4Error::IRQHandlerAck(e))
        }
    }
}

mod private {
    pub trait SealedIRQSetState {}
    impl SealedIRQSetState for super::irq_state::Unset {}
    impl SealedIRQSetState for super::irq_state::Set {}
}
