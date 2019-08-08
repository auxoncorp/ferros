#![no_std]

use ferros::userland::RetypeForSetup;

#[repr(C)]
pub struct ProcParams {
    pub number_of_hellos: u32,
    pub data: [u8; 124],
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}
