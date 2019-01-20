use core::convert::{AsMut, AsRef};
use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{
    role, CNodeRole, Cap, CapRights, ChildCNode, Endpoint, Error, IPCBufferToken, LocalCNode,
    LocalCap, Untyped,
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

/// Type-level-locking for a typed view on the IPCBuffer's msg contents
pub struct IPCBufferGuard<'a, T: Sized> {
    data: &'a T,
    ipc_token: IPCBufferToken,
}

impl<'a, T: Sized> IPCBufferGuard<'a, T> {
    fn acquire(ipc_buffer_token: IPCBufferToken) -> Option<Self> {
        // TODO - Move generic size checks to compile-time somehow
        let t_size = core::mem::size_of::<T>();
        unsafe {
            let buffer = &mut *seL4_GetIPCBuffer();
            let buffer_size = core::mem::size_of_val(&buffer.msg);
            if t_size <= buffer_size {
                Some(Self {
                    data: &*(&buffer.msg as *const [usize] as *const T),
                    ipc_token: ipc_buffer_token,
                })
            } else {
                None
            }
        }
    }

    pub fn release(self) -> IPCBufferToken {
        self.ipc_token
    }
}

impl<'a, T> AsRef<T> for IPCBufferGuard<'a, T> {
    fn as_ref(&self) -> &T {
        self.data
    }
}

/// Type-level-locking for a mutable typed view on the IPCBuffer's msg contents
pub struct IPCBufferGuardMut<'a, T: Sized> {
    data: &'a mut T,
    ipc_token: IPCBufferToken,
}

impl<'a, T: Sized> IPCBufferGuardMut<'a, T> {
    fn acquire(ipc_buffer_token: IPCBufferToken) -> Option<Self> {
        // TODO - Move generic size checks to compile-time somehow
        let t_size = core::mem::size_of::<T>();
        unsafe {
            let buffer: &mut seL4_IPCBuffer = &mut *seL4_GetIPCBuffer();
            let buffer_size = core::mem::size_of_val(&buffer.msg);
            if t_size <= buffer_size {
                Some(Self {
                    data: &mut *(&mut buffer.msg as *mut [usize] as *mut T),
                    ipc_token: ipc_buffer_token,
                })
            } else {
                None
            }
        }
    }

    pub fn release(self) -> IPCBufferToken {
        self.ipc_token
    }
}

impl<'a, T> AsMut<T> for IPCBufferGuardMut<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<Req, Rsp> Caller<Req, Rsp, role::Local> {
    pub fn blocking_call<'a>(
        &mut self,
        request: &Req,
        ipc_buffer_token: IPCBufferToken,
    ) -> Result<(IPCBufferGuard<'a, Rsp>), IPCError> {
        let request_size = core::mem::size_of::<Req>();
        let mut write_guard = {
            if let Some(guard) = IPCBufferGuardMut::acquire(ipc_buffer_token) {
                guard
            } else {
                return Err(IPCError::RequestSizeTooBig);
            }
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
                write_guard.as_mut() as *mut Req,
                1,
            );
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
        if let Some(guard) = IPCBufferGuard::acquire(write_guard.release()) {
            Ok(guard)
        } else {
            Err(IPCError::ResponseSizeTooBig)
        }
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
    pub fn reply_recv<F>(self, ipc_buffer_token: IPCBufferToken, f: F) -> Result<Rsp, IPCError>
    where
        F: Fn(&Req) -> Rsp,
    {
        let request_size = core::mem::size_of::<Req>();
        let response_size = core::mem::size_of::<Rsp>();
        let mut request_guard = match IPCBufferGuard::acquire(ipc_buffer_token) {
            Some(g) => g,
            None => return Err(IPCError::RequestSizeTooBig),
        };
        // Do a regular receive to seed our initial value
        let mut msg_info = unsafe {
            seL4_Recv(
                self.endpoint.cptr,
                0 as *const usize as *mut usize, // TODO - consider actually caring about sender
            )
        };

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
            response = f(request_guard.as_ref());

            let (info, ipc_token) = unsafe {
                let response_msg_info = seL4_MessageInfo_new(
                    0,             // label,
                    0,             // capsUnwrapped,
                    0,             // extraCaps,
                    response_size, // length
                );
                let mut response_guard = match IPCBufferGuardMut::acquire(request_guard.release()) {
                    Some(g) => g,
                    None => return Err(IPCError::ResponseSizeTooBig),
                };
                core::ptr::copy_nonoverlapping(&response as *const Rsp, response_guard.as_mut(), 1);
                (
                    seL4_ReplyRecv(
                        self.endpoint.cptr,
                        response_msg_info,
                        0 as *const usize as *mut usize,
                    ), // TODO - do we care about sender?
                    response_guard.release(),
                )
            };

            msg_info = info;
            request_guard = match IPCBufferGuard::acquire(ipc_token) {
                Some(g) => g,
                None => return Err(IPCError::RequestSizeTooBig),
            };
        }

        // TODO - Let's get some better piping/handling of error conditions - panic only so far
        // TODO - Consider allowing fn to return Option<Rsp> and if None do Rcv rather than ReplyRecv
    }
}
