#![no_std]
#![recursion_limit = "256"]
#![feature(proc_macro_hygiene)]

extern crate arrayvec;
extern crate generic_array;
extern crate registers;
extern crate selfe_sys;
extern crate typenum;

extern crate cross_queue;
extern crate smart_alloc;

#[macro_use]
pub mod debug;

pub mod alloc;
pub mod arch;
pub mod bootstrap;
pub mod cap;
pub mod error;
pub mod pow;
#[cfg(feature = "test_support")]
pub mod test_support;
pub mod userland;
pub mod vspace;
