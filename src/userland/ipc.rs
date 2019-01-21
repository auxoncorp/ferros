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
    // TODO: Delete local endpoint
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
struct IPCBufferWrapper<'a, Req: Sized, Rsp: Sized> {
    buffer: &'a mut seL4_IPCBuffer,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
}

impl<'a, Req: Sized, Rsp: Sized> IPCBufferWrapper<'a, Req, Rsp> {
    /// Precondition: The input must *not* be a reference to data
    /// actually living in the IPC Buffer.
    unsafe fn unchecked_copy_into_buffer<T: Sized>(&mut self, data: &T) {
        core::ptr::copy_nonoverlapping(
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

    /// Precondition: The input must *not* be a reference to data
    /// actually living in the IPC Buffer.
    pub unsafe fn copy_req_into_buffer(&mut self, request: &Req) {
        self.unchecked_copy_into_buffer(request)
    }

    pub unsafe fn copy_req_from_buffer(&self) -> Req {
        self.unchecked_copy_from_buffer()
    }

    /// Precondition: The input must *not* be a reference to data
    /// actually living in the IPC Buffer.
    unsafe fn copy_rsp_into_buffer(&mut self, response: &Rsp) {
        self.unchecked_copy_into_buffer(response)
    }
    unsafe fn copy_rsp_from_buffer(&mut self) -> Rsp {
        self.unchecked_copy_from_buffer()
    }
}

fn get_ipc_buffer<'a, Req, Rsp>() -> Result<IPCBufferWrapper<'a, Req, Rsp>, IPCError> {
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
    Ok(IPCBufferWrapper {
        buffer,
        _req: PhantomData,
        _rsp: PhantomData,
    })
}

impl<Req, Rsp> Caller<Req, Rsp, role::Local> {
    pub fn blocking_call<'a>(&mut self, request: &Req) -> Result<Rsp, IPCError> {
        let mut ipc_buffer = get_ipc_buffer()?;
        let response_msg_info = unsafe {
            let input_msg_info = seL4_MessageInfo_new(
                0, // label,
                0, // capsUnwrapped,
                0, // extraCaps,
                core::mem::size_of::<Req>(),
            );
            ipc_buffer.copy_req_into_buffer(request);
            seL4_Call(self.endpoint.cptr, input_msg_info)
        };
        let response_msg_length = unsafe {
            seL4_MessageInfo_ptr_get_length(
                &response_msg_info as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
            )
        };
        if response_msg_length != core::mem::size_of::<Rsp>() {
            return Err(IPCError::ResponseSizeMismatch);
        }

        Ok(unsafe { ipc_buffer.copy_rsp_from_buffer() })
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
        let response_size = core::mem::size_of::<Rsp>();
        let mut ipc_buffer = get_ipc_buffer()?;
        // Do a regular receive to seed our initial value
        let mut msg_info = unsafe {
            seL4_Recv(
                self.endpoint.cptr,
                0 as *const usize as *mut usize, // TODO - consider actually caring about sender
            )
        };

        let mut response = unsafe { core::mem::zeroed() }; // TODO - replace with Option-swapping
        let mut state = initial_state;
        loop {
            let msg_length = unsafe {
                seL4_MessageInfo_ptr_get_length(
                    &msg_info as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
                )
            };
            if msg_length != request_size {
                // TODO - we should be dropping bad data or replying with an error code
                debug_println!("Request size incoming does not match static size expectation");
                // Note that `continue`'ing from here will essentially cause this process
                // to loop forever, most likely leaving the caller perpetually blocked.
                continue;
            }
            let out = f(unsafe { &ipc_buffer.copy_req_from_buffer() }, state);
            response = out.0;
            state = out.1;

            let info = unsafe {
                let response_msg_info = seL4_MessageInfo_new(
                    0,             // label,
                    0,             // capsUnwrapped,
                    0,             // extraCaps,
                    response_size, // length
                );
                ipc_buffer.copy_rsp_into_buffer(&response);
                seL4_ReplyRecv(
                    self.endpoint.cptr,
                    response_msg_info,
                    0 as *const usize as *mut usize, // TODO - do we care about sender?
                )
            };

            msg_info = info;
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

    unimplemented!()
}

pub struct FaultSource<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
    _role: PhantomData<Role>,
}

pub struct FaultSink<Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _role: PhantomData<Role>,
}
