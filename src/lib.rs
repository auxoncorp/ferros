#![no_std]

#![cfg_attr(feature = "alloc", feature(alloc))]

#[cfg(all(feature = "alloc"))]
#[macro_use]
extern crate alloc;

extern crate sel4_sys;

#[cfg(all(feature = "test"))]
extern crate proptest;

#[cfg(feature = "test")]
pub mod fel4_test;

#[cfg(feature = "KernelPrinting")]
use sel4_sys::DebugOutHandle;

macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}

pub fn run() {
    debug_println!("\nhello from a feL4 app!\n");
}
