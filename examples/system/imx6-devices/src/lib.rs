#![no_std]

#[macro_use]
pub extern crate bounded_registers;
#[macro_use]
pub extern crate typenum;

use typenum::{op, U1, U12};

/// 4KB pages
pub type PageBytes = op!(U1 << U12);

pub mod ecspi1;
pub mod enet;
pub mod gpio;
pub mod gpt;
pub mod iomuxc;
pub mod ocotp;
pub mod uart1;
pub mod wdog;
