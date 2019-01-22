use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{
    role, CNodeRole, Cap, CapRights, ChildCNode, Endpoint, Error, LocalCNode, LocalCap, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U4};

// TODO - improve names and document variants
#[derive(Debug)]
pub enum IPCError {
    RequestSizeTooBig,
    ResponseSizeTooBig,
    ResponseSizeMismatch,
    RequestSizeMismatch,
}

/// Fastpath call channel -> given some memory capacity (untyped) and two child cnodes,
/// (a parent cnode??)
/// create an endpoint locally, copy it to both child cnodes (with the appropriate permissions)
/// delete the local copy?
/// and produce two objects out, one for calling, one for receiving-and-responding
pub fn call_channel<
    ScratchFreeSlots: Unsigned,
    ChildAFreeSlots: Unsigned,
    ChildBFreeSlots: Unsigned,
    Req,
    Rsp,
>(
    local_cnode: LocalCap<LocalCNode<ScratchFreeSlots>>,
    untyped: LocalCap<Untyped<U4>>,
    child_cnode_caller: LocalCap<ChildCNode<ChildAFreeSlots>>,
    child_cnode_responder: LocalCap<ChildCNode<ChildBFreeSlots>>,
) -> Result<
    (
        LocalCap<ChildCNode<Sub1<ChildAFreeSlots>>>,
        LocalCap<ChildCNode<Sub1<ChildBFreeSlots>>>,
        Caller<Req, Rsp, role::Child>,
        Responder<Req, Rsp, role::Child>,
        LocalCap<LocalCNode<Sub1<ScratchFreeSlots>>>,
    ),
    Error,
>
where
    ScratchFreeSlots: Sub<B1>,
    Diff<ScratchFreeSlots, B1>: Unsigned,
    ChildAFreeSlots: Sub<B1>,
    Sub1<ChildAFreeSlots>: Unsigned,
    ChildBFreeSlots: Sub<B1>,
    Sub1<ChildBFreeSlots>: Unsigned,
{
    let (local_endpoint, local_cnode): (LocalCap<Endpoint>, _) = untyped
        .retype_local(local_cnode)
        .expect("could not create local endpoint in call_channel");
    let (child_endpoint_caller, child_cnode_caller) = local_endpoint
        .copy(&local_cnode, child_cnode_caller, CapRights::RWG)
        .expect("Could not copy to child a");
    let (child_endpoint_responder, child_cnode_responder) = local_endpoint
        .copy(&local_cnode, child_cnode_responder, CapRights::RW)
        .expect("Could not copy to child b");

    Ok((
        child_cnode_caller,
        child_cnode_responder,
        Caller {
            endpoint: child_endpoint_caller,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        },
        Responder {
            endpoint: child_endpoint_responder,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        },
        local_cnode,
    ))
}

#[derive(Debug)]
pub struct Caller<Req: Sized, Rsp: Sized, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
    _role: PhantomData<Role>,
}

/// Internal convenience for working with IPC Buffer instances
struct IPCBuffer<'a, Req: Sized, Rsp: Sized> {
    buffer: &'a mut seL4_IPCBuffer,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
}

impl<'a, Req: Sized, Rsp: Sized> IPCBuffer<'a, Req, Rsp> {
    unsafe fn unchecked_copy_into_buffer<T: Sized>(&mut self, data: &T) {
        core::ptr::copy(
            data as *const T,
            &self.buffer.msg as *const [usize] as *const T as *mut T,
            1,
        );
    }
    unsafe fn unchecked_copy_from_buffer<T: Sized>(&self) -> T {
        let mut data = core::mem::zeroed();
        core::ptr::copy_nonoverlapping(
            &self.buffer.msg as *const [usize] as *const T,
            &mut data as *mut T,
            1,
        );
        data
    }

    pub fn copy_req_into_buffer(&mut self, request: &Req) {
        unsafe { self.unchecked_copy_into_buffer(request) }
    }

    pub fn copy_req_from_buffer(&self) -> Req {
        unsafe { self.unchecked_copy_from_buffer() }
    }

    fn copy_rsp_into_buffer(&mut self, response: &Rsp) {
        unsafe { self.unchecked_copy_into_buffer(response) }
    }
    fn copy_rsp_from_buffer(&mut self) -> Rsp {
        unsafe { self.unchecked_copy_from_buffer() }
    }
}

fn get_ipc_buffer<'a, Req, Rsp>() -> Result<IPCBuffer<'a, Req, Rsp>, IPCError> {
    let request_size = core::mem::size_of::<Req>();
    let response_size = core::mem::size_of::<Rsp>();
    let buffer = unsafe {
        let buffer: &mut seL4_IPCBuffer = &mut *seL4_GetIPCBuffer();
        let buffer_size = core::mem::size_of_val(&buffer.msg);
        // TODO - Move this to compile-time somehow
        if request_size > buffer_size {
            return Err(IPCError::RequestSizeTooBig);
        }
        if response_size > buffer_size {
            return Err(IPCError::ResponseSizeTooBig);
        }
        buffer
    };
    Ok(IPCBuffer {
        buffer,
        _req: PhantomData,
        _rsp: PhantomData,
    })
}

fn type_length_message_info<T>() -> seL4_MessageInfo_t {
    unsafe {
        seL4_MessageInfo_new(
            0,                         // label,
            0,                         // capsUnwrapped,
            0,                         // extraCaps,
            core::mem::size_of::<T>(), // length
        )
    }
}

#[derive(Debug)]
pub struct MessageInfo {
    inner: seL4_MessageInfo_t,
}

impl MessageInfo {
    fn copy_inner(&self) -> seL4_MessageInfo_t {
        seL4_MessageInfo_t {
            words: [self.inner.words[0]],
        }
    }
    pub fn label(&self) -> usize {
        unsafe {
            seL4_MessageInfo_ptr_get_label(
                &self.inner as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
            )
        }
    }
    pub fn length(&self) -> usize {
        unsafe {
            seL4_MessageInfo_ptr_get_length(
                &self.inner as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
            )
        }
    }

    fn is_vm_fault(&self) -> bool {
        1i8 == unsafe { seL4_isVMFault_tag(self.copy_inner()) }
    }

    fn is_unknown_syscall(&self) -> bool {
        1i8 == unsafe { seL4_isUnknownSyscall_tag(self.copy_inner()) }
    }

    fn is_user_exception(&self) -> bool {
        1i8 == unsafe { seL4_isUserException_tag(self.copy_inner()) }
    }

    fn is_null_fault(&self) -> bool {
        1i8 == unsafe { seL4_isNullFault_tag(self.copy_inner()) }
    }

    fn is_cap_fault(&self) -> bool {
        1i8 == unsafe { seL4_isCapFault_tag(self.copy_inner()) }
    }
}

impl From<seL4_MessageInfo_t> for MessageInfo {
    fn from(msg: seL4_MessageInfo_t) -> Self {
        MessageInfo { inner: msg }
    }
}

/// TODO - consider dragging more information
/// out of the fault message in the IPC Buffer
/// and populating some inner fields
/// TODO - consider rolling in the sender here, too
#[derive(Debug)]
pub enum Fault {
    VMFault(fault::VMFault),
    UnknownSyscall(fault::UnknownSyscall),
    UserException(fault::UserException),
    NullFault(fault::NullFault),
    CapFault(fault::CapFault),
    UnidentifiedFault(fault::UnidentifiedFault),
}

pub mod fault {
    #[derive(Debug)]
    pub struct VMFault {
        pub sender: usize,
        pub program_counter: usize,
        pub address: usize,
        pub is_instruction_fault: bool,
        pub fault_status_register: usize,
    }
    #[derive(Debug)]
    pub struct UnknownSyscall {
        pub sender: usize,
    }
    #[derive(Debug)]
    pub struct UserException {
        pub sender: usize,
    }
    #[derive(Debug)]
    pub struct NullFault {
        pub sender: usize,
    }
    #[derive(Debug)]
    pub struct CapFault {
        pub sender: usize,
        pub in_receive_phase: bool, // failure occurred during a receive system call
        pub cap_address: usize,
        //lookup_failure_type: LookupFailure //TODO - deeper extraction of the exact cap failure type
    }
    /// Grab bag for faults that don't fit the regular classification
    #[derive(Debug)]
    pub struct UnidentifiedFault {
        pub sender: usize,
    }
}

impl From<(MessageInfo, usize)> for Fault {
    fn from(info_and_sender: (MessageInfo, usize)) -> Self {
        let (info, sender) = info_and_sender;
        let buffer: &mut seL4_IPCBuffer = unsafe { &mut *seL4_GetIPCBuffer() };

        match info {
            _ if info.is_vm_fault() => Fault::VMFault(fault::VMFault {
                sender,
                program_counter: buffer.msg[seL4_VMFault_IP as usize],
                address: buffer.msg[seL4_VMFault_Addr as usize],
                is_instruction_fault: 1 == buffer.msg[seL4_VMFault_PrefetchFault as usize],
                fault_status_register: buffer.msg[seL4_VMFault_FSR as usize],
            }),
            _ if info.is_unknown_syscall() => {
                Fault::UnknownSyscall(fault::UnknownSyscall { sender })
            }
            _ if info.is_user_exception() => Fault::UserException(fault::UserException { sender }),
            _ if info.is_null_fault() => Fault::NullFault(fault::NullFault { sender }),
            _ if info.is_cap_fault() => Fault::CapFault(fault::CapFault {
                sender,
                cap_address: buffer.msg[seL4_CapFault_Addr as usize],
                in_receive_phase: 1usize == buffer.msg[seL4_CapFault_InRecvPhase as usize],
            }),
            _ => Fault::UnidentifiedFault(fault::UnidentifiedFault { sender }),
        }
    }
}

impl<Req, Rsp> Caller<Req, Rsp, role::Local> {
    pub fn blocking_call<'a>(&self, request: &Req) -> Result<Rsp, IPCError> {
        let mut ipc_buffer = get_ipc_buffer()?;
        let msg_info: MessageInfo = unsafe {
            ipc_buffer.copy_req_into_buffer(request);
            seL4_Call(self.endpoint.cptr, type_length_message_info::<Req>())
        }
        .into();
        if msg_info.length() != core::mem::size_of::<Rsp>() {
            return Err(IPCError::ResponseSizeMismatch);
        }
        Ok(ipc_buffer.copy_rsp_from_buffer())
    }
}

#[derive(Debug)]
pub struct Responder<Req: Sized, Rsp: Sized, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
    _role: PhantomData<Role>,
}

impl<Req, Rsp> Responder<Req, Rsp, role::Local> {
    pub fn reply_recv<F>(self, f: F) -> Result<Rsp, IPCError>
    where
        F: Fn(&Req) -> (Rsp),
    {
        self.reply_recv_with_state((), move |req, state| (f(req), state))
    }

    pub fn reply_recv_with_state<F, State>(
        self,
        initial_state: State,
        f: F,
    ) -> Result<Rsp, IPCError>
    where
        F: Fn(&Req, State) -> (Rsp, State),
    {
        let request_size = core::mem::size_of::<Req>();
        let mut ipc_buffer = get_ipc_buffer()?;
        // Do a regular receive to seed our initial value
        let mut msg_info: MessageInfo = unsafe {
            seL4_Recv(
                self.endpoint.cptr,
                0 as *const usize as *mut usize, // TODO - consider actually caring about sender
            )
        }
        .into();

        let mut response = unsafe { core::mem::zeroed() }; // TODO - replace with Option-swapping
        let mut state = initial_state;
        loop {
            if msg_info.length() != request_size {
                // TODO - we should be dropping bad data or replying with an error code
                debug_println!("Request size incoming does not match static size expectation");
                // Note that `continue`'ing from here will essentially cause this process
                // to loop forever, most likely leaving the caller perpetually blocked.
                continue;
            }
            let out = f(&ipc_buffer.copy_req_from_buffer(), state);
            response = out.0;
            state = out.1;

            msg_info = unsafe {
                ipc_buffer.copy_rsp_into_buffer(&response);
                seL4_ReplyRecv(
                    self.endpoint.cptr,
                    type_length_message_info::<Rsp>(),
                    0 as *const usize as *mut usize, // TODO - do we care about sender?
                )
            }
            .into();
        }

        // TODO - Let's get some better piping/handling of error conditions - panic only so far
        // TODO - Consider allowing fn to return Option<Rsp> and if None do Rcv rather than ReplyRecv
    }
}

/// Only supports establishing two child processes where one process will be watching for faults on the other.
/// Requires a separate input signature if we want the local/current thread to be the watcher due to
/// our consuming full instances of the local scratch CNode and the destination CNodes separately in this function.
pub fn setup_fault_endpoint_pair<
    ScratchFreeSlots: Unsigned,
    FaultSourceChildFreeSlots: Unsigned,
    FaultSinkChildFreeSlots: Unsigned,
>(
    local_cnode: LocalCap<LocalCNode<ScratchFreeSlots>>,
    untyped: LocalCap<Untyped<U4>>,
    child_cnode_fault_source: LocalCap<ChildCNode<FaultSourceChildFreeSlots>>,
    child_cnode_fault_sink: LocalCap<ChildCNode<FaultSinkChildFreeSlots>>,
) -> Result<
    (
        LocalCap<ChildCNode<Sub1<FaultSourceChildFreeSlots>>>,
        LocalCap<ChildCNode<Sub1<FaultSinkChildFreeSlots>>>,
        FaultSource<role::Child>,
        FaultSink<role::Child>,
        LocalCap<LocalCNode<Sub1<ScratchFreeSlots>>>,
    ),
    Error,
>
where
    ScratchFreeSlots: Sub<B1>,
    Diff<ScratchFreeSlots, B1>: Unsigned,
    FaultSourceChildFreeSlots: Sub<B1>,
    Sub1<FaultSourceChildFreeSlots>: Unsigned,
    FaultSinkChildFreeSlots: Sub<B1>,
    Sub1<FaultSinkChildFreeSlots>: Unsigned,
{
    let (local_endpoint, local_cnode): (LocalCap<Endpoint>, _) = untyped
        .retype_local(local_cnode)
        .expect("could not create local endpoint in call_channel");
    let (child_endpoint_fault_source, child_cnode_fault_source) = local_endpoint
        .copy(&local_cnode, child_cnode_fault_source, CapRights::RWG)
        .expect("Could not copy to fault source cnode");
    let (child_endpoint_fault_sink, child_cnode_fault_sink) = local_endpoint
        .copy(&local_cnode, child_cnode_fault_sink, CapRights::RW)
        .expect("Could not copy to fault sink cnode");

    // TODO - how should we incorporate badging as a means of allowing a fault-handling/receiving thread
    // to distinguish between the various sources of faults?
    // seems like there is a M:1 problem here that we need to sort out.
    // Possible answer -- keep around a handler to a joint sink endpoint,
    // copy/mutate from that and

    Ok((
        child_cnode_fault_source,
        child_cnode_fault_sink,
        FaultSource {
            endpoint: child_endpoint_fault_source,
            _role: PhantomData,
        },
        FaultSink {
            endpoint: child_endpoint_fault_sink,
            _role: PhantomData,
        },
        local_cnode,
    ))
}

/// The side of a fault endpoint that sends fault messages
#[derive(Debug)]
pub struct FaultSource<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
    _role: PhantomData<Role>,
}

/// The side of a fault endpoint that receives fault messages
#[derive(Debug)]
pub struct FaultSink<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
    _role: PhantomData<Role>,
}

impl FaultSink<role::Local> {
    pub fn wait_for_fault(&self) -> Fault {
        let mut sender: usize = 0;
        let info = unsafe {
            seL4_Recv(
                self.endpoint.cptr,
                &mut sender as *mut usize, // TODO - consider actually caring about sender
            )
        }
        .into();
        (info, sender).into()
    }
}
