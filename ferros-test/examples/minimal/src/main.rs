#![no_std]
#![recursion_limit = "128"]

use ferros::bootstrap::UserImage;
use ferros::cap::{
    ASIDPool, LocalCNode, LocalCNodeSlots, LocalCap, ThreadPriorityAuthority, Untyped,
};
use ferros::test_support::TestOutcome;
use ferros::vspace::ScratchRegion;

use ferros_test::{ferros_test, ferros_test_main};
use typenum::*;

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
fn userimage_parameter(image: &UserImage<ferros::cap::role::Local>) {}

#[ferros_test]
fn vspacescratch_parameter(scratch: &mut ScratchRegion) {}

#[ferros_test]
fn multiple_mixed_parameters(
    untyped: LocalCap<Untyped<U5>>,
    scratch: &mut ScratchRegion,
    slots: LocalCNodeSlots<U5>,
    image: &UserImage<ferros::cap::role::Local>,
    pool: LocalCap<ASIDPool<U1024>>,
    cnode: &LocalCap<LocalCNode>,
) {
}

ferros_test_main!(&[
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
]);
