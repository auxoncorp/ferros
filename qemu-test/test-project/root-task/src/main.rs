#![no_std]
#![recursion_limit = "128"]
#![feature(proc_macro_hygiene)]
#![allow(unused_variables, dead_code)]

extern crate cross_queue;
#[macro_use]
extern crate ferros;
#[macro_use]
extern crate registers;
extern crate selfe_sys;
#[macro_use]
extern crate typenum;

mod call_and_response_loop;
mod child_process_cap_management;
mod child_process_runs;
mod child_thread_runs;
mod dont_tread_on_me;
mod double_door_backpressure;
mod elf_process_runs;
mod fault_or_message_handler;
mod fault_pair;
mod grandkid_process_runs;
mod irq_control_manipulation;
mod memory_read_protection;
mod memory_write_protection;
mod over_register_size_params;
mod reuse_slots;
mod reuse_untyped;
mod root_task_runs;
mod self_hosted_mem_mgmt;
mod shared_page_queue;
mod stack_setup;
mod uart;
mod wutbuddy;

mod resources {
    include! {concat!(env!("OUT_DIR"), "/resources.rs")}
}

extern "C" {
    static _selfe_arc_data_start: u8;
    static _selfe_arc_data_end: usize;
}

use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::alloc::ut_buddy::UTBuddyError;
use ferros::cap::IRQError;
use ferros::cap::RetypeError;
use ferros::error::SeL4Error;
use ferros::userland::{
    FaultManagementError, IPCError, MultiConsumerError, ProcessSetupError, ThreadSetupError,
};
use ferros::vspace::VSpaceError;

#[cfg(not(test_case = "uart"))]
use ferros_test::ferros_test_main;

#[cfg(not(test_case = "uart"))]
ferros_test_main!(&[
    &call_and_response_loop::call_and_response_loop,
    &child_process_cap_management::child_process_cap_management,
    &child_process_runs::child_process_runs,
    &child_thread_runs::child_thread_runs,
    &dont_tread_on_me::dont_tread_on_me,
    &double_door_backpressure::double_door_backpressure,
    &elf_process_runs::elf_process_runs,
    &fault_or_message_handler::fault_or_message_handler,
    &fault_pair::fault_pair,
    &grandkid_process_runs::grandkid_process_runs,
    &irq_control_manipulation::irq_control_manipulation,
    &memory_read_protection::memory_read_protection,
    &memory_write_protection::memory_write_protection,
    &over_register_size_params::over_register_size_params,
    &reuse_slots::reuse_slots,
    &reuse_untyped::reuse_untyped,
    &root_task_runs::root_task_runs,
    &self_hosted_mem_mgmt::self_hosted_mem_mgmt,
    &shared_page_queue::shared_page_queue,
    &stack_setup::stack_setup,
    &wutbuddy::wutbuddy,
]);

#[cfg(test_case = "uart")]
fn main() {
    debug_println!("Starting the test!");
    let bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(bootinfo);
}

#[cfg(test_case = "uart")]
pub fn run(raw_boot_info: &'static selfe_sys::seL4_BootInfo) {
    uart::run(raw_boot_info).expect("run");
    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
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
    ProcessSetupError(ProcessSetupError),
    ThreadSetupError(ThreadSetupError),
    UTBuddyError(UTBuddyError),
    RetypeError(RetypeError),
    TestAssertionFailure(&'static str),
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

impl From<ProcessSetupError> for TopLevelError {
    fn from(e: ProcessSetupError) -> Self {
        TopLevelError::ProcessSetupError(e)
    }
}

impl From<ThreadSetupError> for TopLevelError {
    fn from(e: ThreadSetupError) -> Self {
        TopLevelError::ThreadSetupError(e)
    }
}

impl From<UTBuddyError> for TopLevelError {
    fn from(e: UTBuddyError) -> Self {
        TopLevelError::UTBuddyError(e)
    }
}

impl From<RetypeError> for TopLevelError {
    fn from(e: RetypeError) -> Self {
        TopLevelError::RetypeError(e)
    }
}
