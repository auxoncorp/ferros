#![no_std]

extern crate cross_queue;
extern crate ferros;
extern crate sel4_sys;
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

use ferros::micro_alloc::{self, Error as AllocError, GetUntyped};
use ferros::pow::Pow;
use ferros::userland::{
    role, root_cnode, BootInfo, CNode, CNodeRole, Cap, Endpoint, IPCError, LocalCap,
    MultiConsumerError, RetypeForSetup, SeL4Error, Untyped, VSpaceError,
};
use typenum::operator_aliases::Diff;
use typenum::{U12, U2, U20, U4096, U6};

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

    yield_forever()
}

#[derive(Debug)]
pub enum TopLevelError {
    AllocError(AllocError),
    IPCError(IPCError),
    MultiConsumerError(MultiConsumerError),
    VSpaceError(VSpaceError),
    SeL4Error(SeL4Error),
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
