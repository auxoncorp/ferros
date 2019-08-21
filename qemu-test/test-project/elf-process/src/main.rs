#![no_std]
#![no_main]

use ferros::*;
use ferros::cap::*;
extern crate selfe_runtime;

use elf_process::ProcParams;

static mut MUT_GLOBAL: u32 = 0;

#[no_mangle]
pub extern "C" fn _start(params: ProcParams<role::Local>) -> ! {
    // try to set the mut global, to see that BSS was mapped
    unsafe {
        MUT_GLOBAL = 42;
    }

    params
        .outcome_sender
        .blocking_send(&(params.value == 42))
        .expect("Found value does not match expectations");

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}
