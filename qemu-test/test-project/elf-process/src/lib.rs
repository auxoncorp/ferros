#![no_std]

use ferros::userland::{RetypeForSetup, Sender};
use ferros::cap::*;

pub struct ProcParams<Role: CNodeRole> {
    pub value: usize,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
