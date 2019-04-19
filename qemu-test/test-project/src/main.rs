#![no_std]
#![recursion_limit = "128"]
#![feature(proc_macro_hygiene)]

extern crate cross_queue;
#[macro_use]
extern crate ferros;
#[macro_use]
extern crate registers;
extern crate selfe_sys;
#[macro_use]
extern crate typenum;

use selfe_sys::*;

mod dont_tread_on_me;
mod double_door_backpressure;
#[cfg(dual_process = "true")]
mod dual_process;
mod fault_pair;
#[cfg(single_process = "true")]
mod single_process;
mod uart;

use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::userland::{IPCError, IRQError, MultiConsumerError, SeL4Error, VSpaceError};
use ferros::debug::*;
use core::fmt::Write;
use core::panic::PanicInfo;

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

fn main() {
    debug_println!("Starting the test!");
    let bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(bootinfo);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    sel4_start::debug_panic_handler(&info)
}


pub fn run(raw_boot_info: &'static seL4_BootInfo) {
    #[cfg(single_process = "true")]
    single_process::run(raw_boot_info).expect("single_process run");
    #[cfg(dual_process = "true")]
    dual_process::run(raw_boot_info).expect("dual_process run");
    #[cfg(test_case = "double_door_backpressure")]
    double_door_backpressure::run(raw_boot_info).expect("double_door_backpressure run");
    #[cfg(test_case = "uart")]
    uart::run(raw_boot_info).expect("uart run");
    #[cfg(test_case = "fault_pair")]
    fault_pair::run(raw_boot_info).expect("fault_pair run");
    #[cfg(test_case = "dont_tread_on_me")]
    dont_tread_on_me::run(raw_boot_info).expect("dont_tread_on_me");

    yield_forever()
}

#[derive(Debug)]
pub enum TopLevelError {
    AllocError(AllocError),
    IPCError(IPCError),
    MultiConsumerError(MultiConsumerError),
    VSpaceError(VSpaceError),
    SeL4Error(SeL4Error),
    IRQError(IRQError),
}

impl From<AllocError> for TopLevelError {
    fn from(e: AllocError) -> Self {
        TopLevelError::AllocError(e)
    }
}

impl From<IPCError> for TopLevelError {
    fn from(e: IPCError) -> Self {
        TopLevelError::IPCError(e)
    }
}

impl From<MultiConsumerError> for TopLevelError {
    fn from(e: MultiConsumerError) -> Self {
        TopLevelError::MultiConsumerError(e)
    }
}

impl From<VSpaceError> for TopLevelError {
    fn from(e: VSpaceError) -> Self {
        TopLevelError::VSpaceError(e)
    }
}

impl From<SeL4Error> for TopLevelError {
    fn from(e: SeL4Error) -> Self {
        TopLevelError::SeL4Error(e)
    }
}

impl From<IRQError> for TopLevelError {
    fn from(e: IRQError) -> Self {
        TopLevelError::IRQError(e)
    }
}
