#![no_std]

use ferros::cap::{role, CNodeRole};
use ferros::userland::{Responder, RetypeForSetup};
use imx6_hal::pac::iomuxc::IOMUXC;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Request {
    ConfigureEcSpi1,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Response {
    EcSpi1Configured,
}

#[repr(C)]
pub struct ProcParams<Role: CNodeRole> {
    pub iomuxc: IOMUXC,
    pub responder: Responder<Request, Response, Role>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
