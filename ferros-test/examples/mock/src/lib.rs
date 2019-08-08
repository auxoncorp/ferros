use ferros::bootstrap::*;
use ferros::cap::*;
use ferros::test_support::*;
use ferros::vspace::*;
use ferros_test::ferros_test;
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
fn localcap_slots_before_untyped_parameter(
    slots: LocalCNodeSlots<U5>,
    untyped: LocalCap<Untyped<U5>>,
) {
}

#[ferros_test]
fn localcap_untyped_parameter(untyped: LocalCap<Untyped<U5>>) {}

#[ferros_test]
fn localcnodeslots_parameter(slots: LocalCNodeSlots<U5>) {}

#[ferros_test]
fn localcap_asidpool_parameter(slots: LocalCap<ASIDPool<U1024>>) {}

#[ferros_test]
fn localcap_asidpool_smaller_than_max(slots: LocalCap<ASIDPool<U512>>) {}

#[ferros_test]
fn localcap_localcnode_parameter(node: &LocalCap<LocalCNode>) {}

#[ferros_test]
fn localcap_threadpriorityauthority_parameter(tpa: &LocalCap<ThreadPriorityAuthority>) {}

#[ferros_test]
fn userimage_parameter(image: &UserImage<ferros::cap::role::Local>) {}

#[ferros_test]
fn vspacescratch_parameter(scratch: &mut ScratchRegion) {}

#[ferros_test]
fn vspace_paging_root_parameter(vspace_paging_root: &LocalCap<ferros::arch::PagingRoot>) {}

