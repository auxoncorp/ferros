#![no_std]
#![recursion_limit = "128"]

#[macro_use]
extern crate ferros;

use core::panic::PanicInfo;
use ferros::test_support::{execute_tests, Resources, RunTest, TestOutcome, TestSetupError};
use ferros::userland::{
    ASIDPool, LocalCNode, LocalCNodeSlots, LocalCap, ThreadPriorityAuthority, Untyped, UserImage,
    VSpaceScratchSlice,
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
fn zero_parameters_returns_testoutcome_success() -> TestOutcome {
    TestOutcome::Success
}

#[ferros_test]
fn zero_parameters_returns_testoutcome_failure() -> TestOutcome {
    TestOutcome::Failure
}

#[ferros_test]
fn zero_parameters_returns_result_ok() -> Result<(), ()> {
    Ok(())
}

#[ferros_test]
fn zero_parameters_returns_result_err() -> Result<(), ()> {
    Err(())
}

#[ferros_test]
fn zero_parameters_returns_unit() -> () {}

#[ferros_test]
fn localcap_untyped_parameter(untyped: LocalCap<Untyped<U5>>) {}

#[ferros_test]
fn localcnodeslots_parameter(slots: LocalCNodeSlots<U5>) {}

#[ferros_test]
fn localcap_asidpool_parameter(pool: LocalCap<ASIDPool<U1024>>) {}

#[ferros_test]
fn localcap_asidpool_parameter_smaller_than_max(slots: LocalCap<ASIDPool<U512>>) {}

#[ferros_test]
fn localcap_localcnode_parameter(cnode: &LocalCap<LocalCNode>) {}

#[ferros_test]
fn localcap_threadpriorityauthority_parameter(tpa: &LocalCap<ThreadPriorityAuthority>) {}

#[ferros_test]
fn userimage_parameter(image: &UserImage<ferros::userland::role::Local>) {}

#[ferros_test]
fn vspacescratch_parameter(scratch: &mut VSpaceScratchSlice<ferros::userland::role::Local>) {}

#[ferros_test]
fn multiple_mixed_parameters(
    untyped: LocalCap<Untyped<U5>>,
    scratch: &mut VSpaceScratchSlice<ferros::userland::role::Local>,
    slots: LocalCNodeSlots<U5>,
    image: &UserImage<ferros::userland::role::Local>,
    pool: LocalCap<ASIDPool<U1024>>,
    cnode: &LocalCap<LocalCNode>,
) {
}

fn main() {
    debug_println!("\n\n");
    let bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(bootinfo).expect("Test setup failure");
    yield_forever()
}

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TestSetupError> {
    let (mut resources, reporter) = Resources::with_debug_reporting(raw_boot_info)?;
    execute_tests(
        reporter,
        resources.as_mut_ref(),
        &[
            &zero_parameters,
            &zero_parameters_returns_result_ok,
            &zero_parameters_returns_result_err,
            &zero_parameters_returns_testoutcome_success,
            &zero_parameters_returns_testoutcome_failure,
            &zero_parameters_returns_unit,
            &localcap_asidpool_parameter,
            &localcap_asidpool_parameter_smaller_than_max,
            &localcap_localcnode_parameter,
            &localcap_threadpriorityauthority_parameter,
            &localcap_untyped_parameter,
            &localcnodeslots_parameter,
            &userimage_parameter,
            &vspacescratch_parameter,
        ],
    )?;
    Ok(())
}

fn static_assertion_checks() {
    let _: &RunTest = &zero_parameters;
    let _: &RunTest = &zero_parameters_returns_result_ok;
    let _: &RunTest = &zero_parameters_returns_result_err;
    let _: &RunTest = &zero_parameters_returns_testoutcome_success;
    let _: &RunTest = &zero_parameters_returns_testoutcome_failure;
    let _: &RunTest = &zero_parameters_returns_unit;
    let _: &RunTest = &localcap_asidpool_parameter;
    let _: &RunTest = &localcap_asidpool_parameter_smaller_than_max;
    let _: &RunTest = &localcap_localcnode_parameter;
    let _: &RunTest = &localcap_threadpriorityauthority_parameter;
    let _: &RunTest = &localcap_untyped_parameter;
    let _: &RunTest = &localcnodeslots_parameter;
    let _: &RunTest = &userimage_parameter;
    let _: &RunTest = &vspacescratch_parameter;
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    sel4_start::debug_panic_handler(&info)
}
