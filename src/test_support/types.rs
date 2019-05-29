use typenum::*;

use crate::alloc::micro_alloc::Error as AllocError;
use crate::bootstrap::*;
use crate::cap::*;
use crate::error::SeL4Error;
use crate::pow::Pow;
use crate::userland::*;
use crate::vspace::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestOutcome {
    Success,
    Failure,
}

pub type MaxTestUntypedSize = U27;
pub type MaxTestCNodeSlots = Pow<U16>;
pub type MaxTestASIDPoolSize = crate::arch::ASIDPoolSize;
pub type RunTest = Fn(
    LocalCNodeSlots<MaxTestCNodeSlots>,
    LocalCap<Untyped<MaxTestUntypedSize>>,
    LocalCap<ASIDPool<MaxTestASIDPoolSize>>,
    &mut VSpaceScratchSlice<role::Local>,
    &LocalCap<LocalCNode>,
    &LocalCap<ThreadPriorityAuthority>,
    &UserImage<role::Local>,
) -> (&'static str, TestOutcome);

pub trait TestReporter {
    fn report(&mut self, test_name: &'static str, outcome: TestOutcome);

    fn summary(&mut self, passed: u32, failed: u32);
}

#[derive(Debug)]
pub enum TestSetupError {
    InitialUntypedNotFound { bit_size: usize },
    AllocError(AllocError),
    SeL4Error(SeL4Error),
}

impl From<AllocError> for TestSetupError {
    fn from(e: AllocError) -> Self {
        TestSetupError::AllocError(e)
    }
}

impl From<SeL4Error> for TestSetupError {
    fn from(e: SeL4Error) -> Self {
        TestSetupError::SeL4Error(e)
    }
}

#[derive(Debug)]
pub struct GenericTestParams<Role: CNodeRole> {
    slots: Cap<CNodeSlotsData<MaxTestCNodeSlots, Role>, Role>,
    untyped: Cap<Untyped<MaxTestUntypedSize>, Role>,
    asid_pool: Cap<ASIDPool<MaxTestASIDPoolSize>, Role>,
    scratch: VSpaceScratchSlice<Role>,
    cnode: Cap<CNode<Role>, Role>,
    thread_authority: Cap<ThreadPriorityAuthority, Role>,
    maybe_user_image: Option<UserImage<Role>>,
}

impl RetypeForSetup for GenericTestParams<role::Local> {
    type Output = GenericTestParams<role::Child>;
}
