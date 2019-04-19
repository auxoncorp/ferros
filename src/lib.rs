#![no_std]
// Necessary to mark as not-Send or not-Sync
#![feature(optin_builtin_traits)]
#![feature(associated_type_defaults)]
#![recursion_limit = "128"]
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

pub mod drivers;

pub mod alloc;
pub mod arch;
pub mod config;
pub mod pow;
pub mod userland;
