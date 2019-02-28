#![no_std]
#![recursion_limit = "128"]

extern crate cross_queue;
extern crate ferros;
#[macro_use]
extern crate registers;
extern crate sel4_sys;
#[macro_use]
extern crate typenum;

use sel4_sys::*;

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

mod double_door_backpressure;
#[cfg(dual_process = "true")]
mod dual_process;
#[cfg(single_process = "true")]
mod single_process;
mod uart;

use ferros::micro_alloc::Error as AllocError;
use ferros::userland::{IPCError, IRQError, MultiConsumerError, SeL4Error, VSpaceError};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
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
