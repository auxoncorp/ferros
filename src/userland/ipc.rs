use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{
    role, CNodeRole, Cap, CapRights, ChildCNode, Endpoint, Error,
    LocalCNode, LocalCap, Untyped,
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
    // TODO - revisit CapRights selection, we need to clamp this down!
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

impl<Req, Rsp> Caller<Req, Rsp, role::Local> {
    pub fn blocking_call(&mut self, request: &Req) -> Result<Rsp, IPCError> {
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
        let response_msg_info = unsafe {
            let input_msg_info = seL4_MessageInfo_new(
                0, // label,
                0, // capsUnwrapped,
                0, // extraCaps,
                request_size,
            );
            core::ptr::copy_nonoverlapping(
                request as *const Req,
                &mut buffer.msg as *mut [usize] as *mut Req,
                1,
            );
            seL4_Call(self.endpoint.cptr, input_msg_info)
        };
        let response_msg_length = unsafe {
            seL4_MessageInfo_ptr_get_length(
                &response_msg_info as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t,
            )
        };
        if response_msg_length != response_size {
            return Err(IPCError::ResponseSizeMismatch);
        }
        // TODO - consider replacing with Option swapping
        let mut response = unsafe { core::mem::zeroed() };
        unsafe {
            core::ptr::copy_nonoverlapping(
                &buffer.msg as *const [usize] as *const Rsp,
                &mut response as *mut Rsp,
                1,
            );
        }
        Ok(response)
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
    // TODO - version accepting for state transfer and mutation
    pub fn reply_recv<F>(self, f: F) -> Result<Rsp, IPCError>
    where
        F: Fn(&Req) -> Rsp,
    {
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

        // Do a regular receive to seed our initial value
        let mut msg_info = unsafe {
            seL4_Recv(
                self.endpoint.cptr,
                0 as *const usize as *mut usize, // TODO - consider actually caring about sender
            )
        };

        let mut request = unsafe { core::mem::zeroed() }; // TODO - replace with Option-swapping
        let mut response = unsafe { core::mem::zeroed() }; // TODO - replace with Option-swapping
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
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &buffer.msg as *const [usize] as *const Req,
                    &mut request as *mut Req,
                    1,
                );
            }
            response = f(&request);

            msg_info = unsafe {
                let response_msg_info = seL4_MessageInfo_new(
                    0,             // label,
                    0,             // capsUnwrapped,
                    0,             // extraCaps,
                    response_size, // length
                );
                core::ptr::copy_nonoverlapping(
                    &response as *const Rsp,
                    &mut buffer.msg as *mut [usize] as *mut Rsp,
                    1,
                );
                seL4_ReplyRecv(
                    self.endpoint.cptr,
                    response_msg_info,
                    0 as *const usize as *mut usize,
                ) // TODO - do we care about sender?
            };
        }

        // TODO - Let's get some better piping/handling of error conditions - panic only so far
        // TODO - Consider allowing fn to return Option<Rsp> and if None do Rcv rather than ReplyRecv
    }
}
