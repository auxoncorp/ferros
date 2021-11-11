use ferros::alloc::micro_alloc::Error as AllocError;
use ferros::alloc::ut_buddy::UTBuddyError;
use ferros::cap::IRQError;
use ferros::cap::RetypeError;
use ferros::error::SeL4Error;
use ferros::userland::{FaultManagementError, IPCError, MultiConsumerError, ProcessSetupError};
use ferros::vspace::VSpaceError;

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
