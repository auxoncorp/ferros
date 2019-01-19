use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{
    role, CNode, CNodeRole, Cap, CapType, ChildCNode, DirectRetype, Endpoint, Error, LocalCNode,
    LocalCap, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U4};

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
        LocalCap<LocalCNode<Sub1<ScratchFreeSlots>>>,
        LocalCap<ChildCNode<Sub1<ChildAFreeSlots>>>,
        LocalCap<ChildCNode<Sub1<ChildBFreeSlots>>>,
        Caller<Req, Rsp, role::Child>,
        Responder<Req, Rsp, role::Child>,
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
    // TODO - revisit CapRights selection
    let (child_endpoint_a, child_cnode_a) = local_endpoint
        .copy(&local_cnode, child_cnode_caller, unsafe {
            seL4_CapRights_new(0, 1, 1)
        })
        .expect("Could not copy to child a");
    let (child_endpoint_b, child_cnode_b) = local_endpoint
        .copy(&local_cnode, child_cnode_responder, unsafe {
            seL4_CapRights_new(0, 1, 1)
        })
        .expect("Could not copy to child b");

    Ok((
        local_cnode,
        child_cnode_a,
        child_cnode_b,
        Caller {
            endpoint: child_endpoint_a,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        },
        Responder {
            endpoint: child_endpoint_b,
            _req: PhantomData,
            _rsp: PhantomData,
            _role: PhantomData,
        },
    ))
}

pub struct Caller<Req, Rsp, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
    _role: PhantomData<Role>,
}

pub struct Responder<Req, Rsp, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _req: PhantomData<Req>,
    _rsp: PhantomData<Rsp>,
    _role: PhantomData<Role>,
}
