#[derive(Debug)]
pub enum SeL4Error {
    UntypedRetype(KernelError),
    TCBConfigure(KernelError),
    PageTableMap(KernelError),
    PageUpperDirectoryMap(KernelError),
    PageDirectoryMap(KernelError),
    ASIDPoolAssign(KernelError),
    PageMap(KernelError),
    PageUnmap(KernelError),
    CNodeCopy(KernelError),
    CNodeMint(KernelError),
    TCBWriteRegisters(KernelError),
    TCBReadRegisters(KernelError),
    TCBSetPriority(KernelError),
    TCBResume(KernelError),
    CNodeMutate(KernelError),
    CNodeMove(KernelError),
    CNodeDelete(KernelError),
    IRQControlGet(KernelError),
    IRQHandlerSetNotification(KernelError),
    IRQHandlerAck(KernelError),
    GetPageAddr(KernelError),
    PageCleanInvalidateData(KernelError),
    CNodeRevoke(KernelError),
    VCPUInjectIRQ(KernelError),
    VCPUReadRegisters(KernelError),
    VCPUWriteRegisters(KernelError),
    VCPUBindTcb(KernelError),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KernelError {
    InvalidArgument,
    InvalidCapability,
    IllegalOperation,
    RangeError,
    AlignmentError,
    FailedLookup,
    TruncatedMessage,
    DeleteFirst,
    RevokeFirst,
    NotEnoughMemory,
    /// A kernel error code that was not recognized
    UnknownError(u32),
}

pub trait ErrorExt {
    fn as_result(self) -> Result<(), KernelError>;
}

impl ErrorExt for selfe_sys::seL4_Error {
    fn as_result(self) -> Result<(), KernelError> {
        match self {
            selfe_sys::seL4_Error_seL4_NoError => Ok(()),
            selfe_sys::seL4_Error_seL4_InvalidArgument => Err(KernelError::InvalidArgument),
            selfe_sys::seL4_Error_seL4_InvalidCapability => Err(KernelError::InvalidCapability),
            selfe_sys::seL4_Error_seL4_IllegalOperation => Err(KernelError::IllegalOperation),
            selfe_sys::seL4_Error_seL4_RangeError => Err(KernelError::RangeError),
            selfe_sys::seL4_Error_seL4_AlignmentError => Err(KernelError::AlignmentError),
            selfe_sys::seL4_Error_seL4_FailedLookup => Err(KernelError::FailedLookup),
            selfe_sys::seL4_Error_seL4_TruncatedMessage => Err(KernelError::TruncatedMessage),
            selfe_sys::seL4_Error_seL4_DeleteFirst => Err(KernelError::DeleteFirst),
            selfe_sys::seL4_Error_seL4_RevokeFirst => Err(KernelError::RevokeFirst),
            selfe_sys::seL4_Error_seL4_NotEnoughMemory => Err(KernelError::NotEnoughMemory),
            unknown => Err(KernelError::UnknownError(unknown)),
        }
    }
}
