use core::marker::PhantomData;

use typenum::consts::{True, U256};
use typenum::{IsLess, Unsigned};

use crate::cap::{CapType, Movable};

// TODO - consider moving IRQ code allocation tracking to compile-time,
// which may be feasible since we treat IRQControl as a global
// singleton.
// The goal of such tracking is to prevent accidental double-binding to a single IRQ
pub struct IRQControl {
    pub(crate) known_handled: [bool; 256],
}

impl CapType for IRQControl {}

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

/// Whether or not an IRQ Handle has been set to a particular Notification
pub trait IRQSetState {}

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
