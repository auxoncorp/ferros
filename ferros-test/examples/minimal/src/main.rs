#![no_std]
#![recursion_limit = "128"]
//#![feature(proc_macro_hygiene)]

use core::panic::PanicInfo;
use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::test_support::{RunTest, RunnableTest, TestOutcome};
use ferros::userland::{
    FaultManagementError, IPCError, IRQError, MultiConsumerError, SeL4Error, VSpaceError,
};
use ferros_test::ferros_test;
use selfe_sys::*;

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

fn main() {
    let bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(bootinfo);
}

fn static_assertion_checks() {
    let _: &RunTest = &zero_parameters;
}

#[ferros_test]
fn zero_parameters() {}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    sel4_start::debug_panic_handler(&info)
}

pub fn run(_raw_boot_info: &'static seL4_BootInfo) {
    static_assertion_checks();
    yield_forever()
}

#[derive(Debug)]
pub enum TopLevelError {
    AllocError(AllocError),
    IPCError(IPCError),
    VSpaceError(VSpaceError),
    SeL4Error(SeL4Error),
    FaultManagementError(FaultManagementError),
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

impl From<FaultManagementError> for TopLevelError {
    fn from(e: FaultManagementError) -> Self {
        TopLevelError::FaultManagementError(e)
    }
}
