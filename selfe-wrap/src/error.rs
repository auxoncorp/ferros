#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct APIError {
    pub method: APIMethod,
    pub error: SeL4Error,
}

impl APIError {
    pub fn new(method: APIMethod, error: SeL4Error) -> Self {
        APIError { method, error }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum APIMethod {
    Send,
    Recv,
    Call,
    Reply,
    NBSend,
    ReplyRecv,
    NBRecv,
    Yield,
    Signal,
    Wait,
    Poll,
    Benchmark(BenchmarkMethod),
    Debug(DebugMethod),
    CNode(CNodeMethod),
    DomainSetSet,
    IRQControlGet,
    IRQControlGetIOAPIC, // X86
    IRQControlGetMSI,    // X86

    IRQHandler(IRQHandlerMethod),
    TCB(TCBMethod),
    UntypedRetype,
    ASIDControlMakePool,
    ASIDPoolAssign,
    Page(PageMethod),
    IOPageTableMap,   // X86 and Arm and Riscv
    IOPageTableUnmap, // X86 and Arm and Riscv
    PageTableMap,     // X86 and Arm and Riscv
    PageTableUnmap,   // X86 and Arm and Riscv

    EPTPDMap,             // X86
    EPTPDUnmap,           // X86
    EPTPDPTMap,           // X86
    EPTPDPTUnmap,         // X86
    EPTPTMap,             // X86
    EPTPTUnmap,           // X86
    IOPortControlIssue,   // X86
    IOPort(IOPortMethod), // X86
    VCPU(VCPUMethod),     // X86

    PageDirectoryMap,      // Arm
    PageDirectoryUnmap,    // Arm
    PageUpperDirectoryMap, // Arm
    PageUpperDirectoryUnmap, // Arm

                           // TODO - X86_VMEnter syscall
                           // TODO - more other arch-independent object methods
                           // TODO - arch-specific object methods
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum PageMethod {
    Map,
    Unmap,
    Remap,
    GetAddress,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum VCPUMethod {
    DisableIOPort,
    EnableIOPort,   // X86
    ReadVMCS,       // X86
    SetTCB,         // X86 and ARM
    WriteRegisters, // X86
    WriteVMCS,      // X86
    InjectIRQ,      // ARM
    ReadRegs,       // ARM - silly name difference from X86
    WriteRegs,      // ARM - silly name difference from X86
}

// X86 only
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum IOPortMethod {
    In16,  // X86
    In32,  // X86
    In8,   // X86
    Out16, // X86
    Out32, // X86
    Out8,  // X86
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum BenchmarkMethod {
    ResetLog,
    FinalizeLog,
    SetLogBuffer,
    NullSyscall,
    FlushCaches,
    GetThreadUtilization,
    ResetThreadUtilization,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum DebugMethod {
    PutChar,
    DumpScheduler,
    Halt,
    Snapshot,
    CapIdentify,
    NameThread,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum CNodeMethod {
    CancelBadgedSends,
    Copy,
    Delete,
    Mint,
    Move,
    Mutate,
    Revoke,
    Rotate,
    SaveCaller,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum IRQHandlerMethod {
    Ack,
    Clear,
    SetNotification,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum TCBMethod {
    BindNotification,
    ConfigureSingleStepping,
    Configure,
    CopyRegisters,
    GetBreakpoint,
    ReadRegisters,
    Resume,
    SetBreakpoint,
    SetAffinity,
    SetIPCBuffer,
    SetMCPriority,
    SetPriority,
    SetSchedParams,
    SetSpace,
    SetTLSBase,
    Suspend,
    UnbindNotification,
    UnsetBreakpoint,
    WriteRegisters,

    SetEPTRoot, // X86
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeL4Error {
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
    fn as_result(self) -> Result<(), SeL4Error>;
}

impl ErrorExt for selfe_sys::seL4_Error {
    fn as_result(self) -> Result<(), SeL4Error> {
        match self {
            selfe_sys::seL4_Error_seL4_NoError => Ok(()),
            selfe_sys::seL4_Error_seL4_InvalidArgument => Err(SeL4Error::InvalidArgument),
            selfe_sys::seL4_Error_seL4_InvalidCapability => Err(SeL4Error::InvalidCapability),
            selfe_sys::seL4_Error_seL4_IllegalOperation => Err(SeL4Error::IllegalOperation),
            selfe_sys::seL4_Error_seL4_RangeError => Err(SeL4Error::RangeError),
            selfe_sys::seL4_Error_seL4_AlignmentError => Err(SeL4Error::AlignmentError),
            selfe_sys::seL4_Error_seL4_FailedLookup => Err(SeL4Error::FailedLookup),
            selfe_sys::seL4_Error_seL4_TruncatedMessage => Err(SeL4Error::TruncatedMessage),
            selfe_sys::seL4_Error_seL4_DeleteFirst => Err(SeL4Error::DeleteFirst),
            selfe_sys::seL4_Error_seL4_RevokeFirst => Err(SeL4Error::RevokeFirst),
            selfe_sys::seL4_Error_seL4_NotEnoughMemory => Err(SeL4Error::NotEnoughMemory),
            unknown => Err(SeL4Error::UnknownError(unknown)),
        }
    }
}
