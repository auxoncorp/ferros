#[derive(Debug)]
pub enum SeL4Error {
    UntypedRetype(KernelError),
    TCBConfigure(u32),
    PageTableMap(u32),
    PageUpperDirectoryMap(u32),
    PageDirectoryMap(u32),
    UnmapPageTable(u32),
    ASIDPoolAssign(u32),
    PageMap(u32),
    PageUnmap(u32),
    CNodeCopy(u32),
    CNodeMint(u32),
    TCBWriteRegisters(u32),
    TCBSetPriority(u32),
    TCBResume(u32),
    CNodeMutate(u32),
    CNodeMove(u32),
    CNodeDelete(u32),
    IRQControlGet(u32),
    IRQHandlerSetNotification(u32),
    IRQHandlerAck(u32),
    GetPageAddr(u32),
    PageCleanInvalidateData(u32),
    CNodeRevoke(u32),
}

#[derive(Debug)]
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
