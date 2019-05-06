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

mod call_and_response_loop;
mod child_process_cap_management;
mod child_process_runs;
mod dont_tread_on_me;
mod double_door_backpressure;
mod fault_pair;
mod memory_read_protection;
mod memory_write_protection;
mod over_register_size_params;
mod reuse_untyped;
mod root_task_runs;
mod shared_page_queue;
mod uart;

use core::panic::PanicInfo;
use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::userland::{
    FaultManagementError, IPCError, IRQError, MultiConsumerError, SeL4Error, VSpaceError,
};

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
    #[cfg(test_case = "call_and_response_loop")]
    call_and_response_loop::run(raw_boot_info).expect("run");
    #[cfg(test_case = "child_process_cap_management")]
    child_process_cap_management::run(raw_boot_info).expect("run");
    #[cfg(test_case = "child_process_runs")]
    child_process_runs::run(raw_boot_info).expect("run");
    #[cfg(test_case = "dont_tread_on_me")]
    dont_tread_on_me::run(raw_boot_info).expect("run");
    #[cfg(test_case = "double_door_backpressure")]
    double_door_backpressure::run(raw_boot_info).expect("run");
    #[cfg(test_case = "fault_pair")]
    fault_pair::run(raw_boot_info).expect("run");
    #[cfg(test_case = "memory_read_protection")]
    memory_read_protection::run(raw_boot_info).expect("run");
    #[cfg(test_case = "memory_write_protection")]
    memory_write_protection::run(raw_boot_info).expect("run");
    #[cfg(test_case = "over_register_size_params")]
    over_register_size_params::run(raw_boot_info).expect("run");
    #[cfg(test_case = "reuse_untyped")]
    reuse_untyped::run(raw_boot_info).expect("run");
    #[cfg(test_case = "root_task_runs")]
    root_task_runs::run(raw_boot_info).expect("run");
    #[cfg(test_case = "shared_page_queue")]
    shared_page_queue::run(raw_boot_info).expect("run");
    #[cfg(test_case = "uart")]
    uart::run(raw_boot_info).expect("run");

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

impl From<FaultManagementError> for TopLevelError {
    fn from(e: FaultManagementError) -> Self {
        TopLevelError::FaultManagementError(e)
    }
}
