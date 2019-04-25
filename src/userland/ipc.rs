use crate::userland::cap::DirectRetype;
use crate::userland::{
    role, Badge, CNodeRole, Cap, CapRights, ChildCNodeSlot, ChildCap, Endpoint, LocalCNode,
    LocalCNodeSlot, LocalCap, SeL4Error, Untyped,
};
use core::marker::PhantomData;
use selfe_sys::*;

#[derive(Debug)]
pub enum IPCError {
    RequestSizeTooBig,
    ResponseSizeTooBig,
    ResponseSizeMismatch,
    RequestSizeMismatch,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for IPCError {
    fn from(s: SeL4Error) -> Self {
        IPCError::SeL4Error(s)
    }
}

#[derive(Debug)]
pub enum FaultManagementError {
    SelfFaultHandlingForbidden,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for FaultManagementError {
    fn from(s: SeL4Error) -> Self {
        FaultManagementError::SeL4Error(s)
    }
}

pub struct IpcSetup<'a, Req, Rsp> {
    endpoint: LocalCap<Endpoint>,
    endpoint_cnode: &'a LocalCap<LocalCNode>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
}

/// Fastpath call channel -> given some memory capacity and two child cnodes,
/// create an endpoint locally, copy it to the responder process cnode, and return an
/// IpcSetup to allow connecting callers.
pub fn call_channel<Req: Send + Sync, Rsp: Send + Sync>(
    untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
    local_cnode: &LocalCap<LocalCNode>,
    local_slot: LocalCNodeSlot,
    child_slot: ChildCNodeSlot,
) -> Result<(IpcSetup<Req, Rsp>, Responder<Req, Rsp, role::Child>), IPCError> {
    let _ = IPCBuffer::<Req, Rsp>::new()?; // Check buffer fits Req and Rsp
    let local_endpoint: LocalCap<Endpoint> = untyped.retype(local_slot)?;
    let child_endpoint = local_endpoint.copy(&local_cnode, child_slot, CapRights::RW)?;

    Ok((
        IpcSetup {
            endpoint: local_endpoint,
            endpoint_cnode: &local_cnode,
            _req: PhantomData,
            _rsp: PhantomData,
        },
        Responder {
            endpoint: child_endpoint,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        },
    ))
}

impl<'a, Req, Rsp> IpcSetup<'a, Req, Rsp> {
    pub fn create_caller(
        &self,
        child_slot: ChildCNodeSlot,
    ) -> Result<Caller<Req, Rsp, role::Child>, IPCError> {
        let child_endpoint =
            self.endpoint
                .copy(&self.endpoint_cnode, child_slot, CapRights::RWG)?;

        Ok(Caller {
            endpoint: child_endpoint,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        })
    }
}

#[derive(Debug)]
pub struct Caller<Req: Sized, Rsp: Sized, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
    _role: PhantomData<Role>,
}

/// Internal convenience for working with IPC Buffer instances
/// *Note:* In a given thread or process, all instances of
/// IPCBuffer wrap a pointer to the very same underlying buffer.
struct IPCBuffer<'a, Req: Sized, Rsp: Sized> {
    buffer: &'a mut seL4_IPCBuffer,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
}

impl<'a, Req: Sized, Rsp: Sized> IPCBuffer<'a, Req, Rsp> {
    /// Don't forget that while this says `new` in the signature,
    /// it is still aliasing the thread-global IPC Buffer pointer
    fn new() -> Result<Self, IPCError> {
        let request_size = core::mem::size_of::<Req>();
        let response_size = core::mem::size_of::<Rsp>();
        let buffer = unchecked_raw_ipc_buffer();
        let buffer_size = core::mem::size_of_val(&buffer.msg);
        // TODO - Move this to compile-time somehow
        if request_size > buffer_size {
            return Err(IPCError::RequestSizeTooBig);
        }
        if response_size > buffer_size {
            return Err(IPCError::ResponseSizeTooBig);
        }
        Ok(IPCBuffer {
            buffer,
            _req: PhantomData,
            _rsp: PhantomData,
        })
    }

    /// Don't forget that while this says `new` in the signature,
    /// it is still aliasing the thread-global IPC Buffer pointer
    ///
    /// Use only when all possible prior paths have conclusively
    /// checked sizing constraints
    unsafe fn unchecked_new() -> Self {
        IPCBuffer {
            buffer: unchecked_raw_ipc_buffer(),
            _req: PhantomData,
            _rsp: PhantomData,
        }
    }

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

fn unchecked_raw_ipc_buffer<'a>() -> &'a mut seL4_IPCBuffer {
    unsafe { &mut *seL4_GetIPCBuffer() }
}

fn type_length_in_words<T>() -> usize {
    let t_bytes = core::mem::size_of::<T>();
    let usize_bytes = core::mem::size_of::<usize>();
    if t_bytes == 0 {
        return 0;
    }
    if t_bytes < usize_bytes {
        return 1;
    }
    let words = t_bytes / usize_bytes;
    let rem = t_bytes % usize_bytes;
    if rem > 0 {
        words + 1
    } else {
        words
    }
}

fn type_length_message_info<T>() -> seL4_MessageInfo_t {
    unsafe {
        seL4_MessageInfo_new(
            0,                                  // label,
            0,                                  // capsUnwrapped,
            0,                                  // extraCaps,
            type_length_in_words::<T>() as u32, // length in words!
        )
    }
}

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
            ) as usize
        }
    }

    /// Length of the message in words, ought to be
    /// less than the length of the IPC Buffer's msg array,
    /// an array of `usize` words.
    fn length_words(&self) -> usize {
        unsafe {
            seL4_MessageInfo_ptr_get_length(
                &self.inner as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
            ) as usize
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
#[derive(Debug)]
pub enum Fault {
    VMFault(fault::VMFault),
    UnknownSyscall(fault::UnknownSyscall),
    UserException(fault::UserException),
    NullFault(fault::NullFault),
    CapFault(fault::CapFault),
    UnidentifiedFault(fault::UnidentifiedFault),
}

impl Fault {
    pub fn sender(&self) -> Badge {
        match self {
            Fault::VMFault(f) => f.sender,
            Fault::UnknownSyscall(f) => f.sender,
            Fault::UserException(f) => f.sender,
            Fault::NullFault(f) => f.sender,
            Fault::CapFault(f) => f.sender,
            Fault::UnidentifiedFault(f) => f.sender,
        }
    }
}

pub mod fault {
    use super::Badge;
    #[derive(Debug)]
    pub struct VMFault {
        pub sender: Badge,
        pub program_counter: usize,
        pub address: usize,
        pub is_instruction_fault: bool,
        pub fault_status_register: usize,
    }
    #[derive(Debug)]
    pub struct UnknownSyscall {
        pub sender: Badge,
    }
    #[derive(Debug)]
    pub struct UserException {
        pub sender: Badge,
    }
    #[derive(Debug)]
    pub struct NullFault {
        pub sender: Badge,
    }
    #[derive(Debug)]
    pub struct CapFault {
        pub sender: Badge,
        pub in_receive_phase: bool,
        pub cap_address: usize,
    }
    /// Grab bag for faults that don't fit the regular classification
    #[derive(Debug)]
    pub struct UnidentifiedFault {
        pub sender: Badge,
    }
}

impl From<(MessageInfo, Badge)> for Fault {
    fn from(info_and_sender: (MessageInfo, Badge)) -> Self {
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
                in_receive_phase: 1 == buffer.msg[seL4_CapFault_InRecvPhase as usize],
            }),
            _ => Fault::UnidentifiedFault(fault::UnidentifiedFault { sender }),
        }
    }
}

impl<Req, Rsp> Caller<Req, Rsp, role::Local> {
    pub fn blocking_call<'a>(&self, request: &Req) -> Result<Rsp, IPCError> {
        // Can safely use unchecked_new because we check sizing during the creation of Caller
        let mut ipc_buffer = unsafe { IPCBuffer::unchecked_new() };
        let msg_info: MessageInfo = unsafe {
            ipc_buffer.copy_req_into_buffer(request);
            seL4_Call(self.endpoint.cptr, type_length_message_info::<Req>())
        }
        .into();
        if msg_info.length_words() != type_length_in_words::<Rsp>() {
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
    pub fn reply_recv<F>(self, mut f: F) -> Result<Rsp, IPCError>
    where
        F: FnMut(&Req) -> (Rsp),
    {
        self.reply_recv_with_state((), move |req, state| (f(req), state))
    }

    pub fn reply_recv_with_state<F, State>(
        self,
        initial_state: State,
        mut f: F,
    ) -> Result<Rsp, IPCError>
    where
        F: FnMut(&Req, State) -> (Rsp, State),
    {
        // Can safely use unchecked_new because we check sizing during the creation of Responder
        let mut ipc_buffer = unsafe { IPCBuffer::unchecked_new() };
        let mut sender_badge: usize = 0;
        // Do a regular receive to seed our initial value
        let mut msg_info: MessageInfo =
            unsafe { seL4_Recv(self.endpoint.cptr, &mut sender_badge as *mut usize) }.into();

        let request_length_in_words = type_length_in_words::<Req>();
        let mut response;
        let mut state = initial_state;
        loop {
            if msg_info.length_words() != request_length_in_words {
                // A wrong-sized message length is an indication of unforeseen or
                // misunderstood kernel operations. Using the checks established in
                // the creation of Caller/Responder sets should prevent the creation
                // of wrong-sized messages through their expected paths.
                //
                // Not knowing what this incoming message is, we drop it and spin-fail the loop.
                // Note that `continue`'ing from here will cause this process
                // to loop forever doing this check with no fresh data, most likely leaving the caller perpetually blocked.
                debug_println!("Request size incoming ({} words) does not match static size expectation ({} words).",
                msg_info.length_words(), request_length_in_words);
                continue;
            }
            let out = f(&ipc_buffer.copy_req_from_buffer(), state);
            response = out.0;
            state = out.1;

            ipc_buffer.copy_rsp_into_buffer(&response);
            msg_info = unsafe {
                seL4_ReplyRecv(
                    self.endpoint.cptr,
                    type_length_message_info::<Rsp>(),
                    &mut sender_badge as *mut usize,
                )
            }
            .into();
        }
    }
}

pub struct FaultSinkSetup {
    // Local pointer to the endpoint, kept around for easy copying
    local_endpoint: LocalCap<Endpoint>,
    // Copy of the same endpoint, set up with the correct rights,
    // living in the CSpace of a child CNode that will become
    // the root of the fault-handling process.
    sink_child_endpoint: ChildCap<Endpoint>,

    // To enable checking whether there is an accidental attempt
    // to wire up a process root CSpace as its own fault handler
    sink_cspace_local_cptr: usize,
}

impl FaultSinkSetup {
    pub fn new(
        local_cnode: &LocalCap<LocalCNode>,
        untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
        endpoint_slot: LocalCNodeSlot,
        fault_sink_slot: ChildCNodeSlot,
    ) -> Result<Self, SeL4Error> {
        let sink_cspace_local_cptr = fault_sink_slot.cptr;

        let local_endpoint: LocalCap<Endpoint> = untyped.retype(endpoint_slot)?;

        let sink_child_endpoint =
            local_endpoint.copy(&local_cnode, fault_sink_slot, CapRights::RW)?;

        Ok(FaultSinkSetup {
            local_endpoint,
            sink_child_endpoint,
            sink_cspace_local_cptr,
        })
    }

    pub fn add_fault_source(
        &self,
        local_cnode: &LocalCap<LocalCNode>,
        fault_source_slot: ChildCNodeSlot,
        badge: Badge,
    ) -> Result<FaultSource<role::Child>, FaultManagementError> {
        if fault_source_slot.cptr == self.sink_cspace_local_cptr {
            return Err(FaultManagementError::SelfFaultHandlingForbidden);
        }

        let child_endpoint_fault_source =
            self.local_endpoint
                .mint_new(local_cnode, fault_source_slot, CapRights::RWG, badge)?;

        Ok(FaultSource {
            endpoint: child_endpoint_fault_source,
        })
    }

    pub fn sink(self) -> FaultSink<role::Child> {
        FaultSink {
            endpoint: self.sink_child_endpoint,
        }
    }
}

/// Only supports establishing two child processes where one process will be watching for faults on the other.
/// Requires a separate input signature if we want the local/current thread to be the watcher due to
/// our consuming full instances of the local scratch CNode and the destination CNodes separately in this function.
pub fn setup_fault_endpoint_pair(
    local_cnode: &LocalCap<LocalCNode>,
    untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
    endpoint_slot: LocalCNodeSlot,
    fault_source_slot: ChildCNodeSlot,
    fault_sink_slot: ChildCNodeSlot,
) -> Result<(FaultSource<role::Child>, FaultSink<role::Child>), FaultManagementError> {
    let setup = FaultSinkSetup::new(&local_cnode, untyped, endpoint_slot, fault_sink_slot)?;
    let fault_source = setup.add_fault_source(&local_cnode, fault_source_slot, Badge::from(0))?;
    Ok((fault_source, setup.sink()))
}

/// The side of a fault endpoint that sends fault messages
#[derive(Debug)]
pub struct FaultSource<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
}

/// The side of a fault endpoint that receives fault messages
#[derive(Debug)]
pub struct FaultSink<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
}

impl FaultSink<role::Local> {
    pub fn wait_for_fault(&self) -> Fault {
        let mut sender: usize = 0;
        let info = unsafe { seL4_Recv(self.endpoint.cptr, &mut sender as *mut usize) }.into();
        (info, Badge::from(sender)).into()
    }
}
