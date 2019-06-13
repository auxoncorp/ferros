use typenum::*;

use crate::alloc::micro_alloc::Error as AllocError;
use crate::bootstrap::*;
use crate::cap::*;
use crate::error::SeL4Error;
use crate::pow::Pow;
use crate::vspace::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestOutcome {
    Success,
    Failure,
}

pub type MaxTestUntypedSize = U27;
pub type MaxTestCNodeSlots = Pow<U17>;
pub type MaxTestASIDPoolSize = crate::arch::ASIDPoolSize;
pub type RunTest = Fn(
    LocalCNodeSlots<MaxTestCNodeSlots>,
    LocalCap<Untyped<MaxTestUntypedSize>>,
    LocalCap<ASIDPool<MaxTestASIDPoolSize>>,
    &mut ScratchRegion<crate::userland::process::StackPageCount>,
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
    VSpaceError(VSpaceError),
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

impl From<VSpaceError> for TestSetupError {
    fn from(e: VSpaceError) -> Self {
        TestSetupError::VSpaceError(e)
    }
}
