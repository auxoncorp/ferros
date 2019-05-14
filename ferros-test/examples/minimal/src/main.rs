#![no_std]
#![recursion_limit = "128"]
//#![feature(proc_macro_hygiene)]

use core::panic::PanicInfo;
use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::test_support::{RunTest, RunnableTest, TestOutcome};
use ferros::userland::{
    CNodeSlots, FaultManagementError, IPCError, IRQError, LocalCNode, LocalCNodeSlots, LocalCap,
    MultiConsumerError, SeL4Error, Untyped, VSpaceError,
};
use ferros_test::ferros_test;
use selfe_sys::*;
use typenum::*;

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

#[ferros_test]
fn zero_parameters() {}

#[ferros_test]
fn zero_parameters_returns_testoutcome() -> TestOutcome {}

#[ferros_test]
fn zero_parameters_returns_result() -> Result<(), ()> {}

#[ferros_test]
fn zero_parameters_returns_unit() -> () {}

#[ferros_test]
fn localcap_untyped_parameter(slots: LocalCap<Untyped<U5>>) {}

#[ferros_test]
fn localcnodeslots_parameter(slots: LocalCNodeSlots<U5>) {}

#[ferros_test]
fn localcap_asidpool_parameter(slots: LocalCap<ASIDPool<U1024>>) {}

#[ferros_test]
fn localcap_localcnode_parameter(slots: LocalCap<LocalCNode>) {}

#[ferros_test]
fn localcap_threadpriorityauthority_parameter(slots: LocalCap<ThreadPriorityAuthority>) {}

#[ferros_test]
fn userimage_parameter(slots: UserImage<ferros::userland::role::Local>) {}

fn main() {
    let bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(bootinfo);
}

fn static_assertion_checks() {
    let _: &RunTest = &zero_parameters;
    let _: &RunTest = &zero_parameters_returns_result;
    let _: &RunTest = &zero_parameters_returns_testoutcome;
    let _: &RunTest = &zero_parameters_returns_unit;
    let _: &RunTest = &localcap_asidpool_parameter;
    let _: &RunTest = &localcap_localcnode_parameter;
    let _: &RunTest = &localcap_threadpriorityauthority_parameter;
    let _: &RunTest = &localcap_untyped_parameter;
    let _: &RunTest = &localcnodeslots_parameter;
    let _: &RunTest = &userimage_parameter;
}

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
