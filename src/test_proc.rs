use core::convert::AsRef;
use crate::pow::Pow;
use crate::userland::{self, role, CNode, CNodeRole, Caller, Cap, IPCBufferToken, Responder};
use typenum::operator_aliases::Diff;
use typenum::{U12, U2};

#[derive(Debug)]
pub struct AdditionRequest {
    a: u32,
    b: u32,
}

#[derive(Debug)]
pub struct AdditionResponse {
    sum: u32,
}

#[derive(Debug)]
pub struct CallerParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub caller: Caller<AdditionRequest, AdditionResponse, Role>,
}

impl userland::RetypeForSetup for CallerParams<role::Local> {
    type Output = CallerParams<role::Child>;
}

#[derive(Debug)]
pub struct ResponderParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub responder: Responder<AdditionRequest, AdditionResponse, Role>,
}

impl userland::RetypeForSetup for ResponderParams<role::Local> {
    type Output = ResponderParams<role::Child>;
}

pub extern "C" fn addition_requester(params_and_ipc: (CallerParams<role::Local>, IPCBufferToken)) {
    let (p, mut ipc_token) = params_and_ipc;
    debug_println!("Inside addition_requester");
    let mut current_sum: u32 = 1;
    let mut caller = p.caller;
    let mut addition_request = AdditionRequest {
        a: current_sum,
        b: current_sum,
    };
    while current_sum < 100 {
        addition_request.a = current_sum;
        addition_request.b = current_sum;
        debug_println!(
            "Q: What is {} + {}?",
            addition_request.a,
            addition_request.b
        );
        match caller.blocking_call(&addition_request, ipc_token) {
            Ok(rsp_guard) => {
                current_sum = rsp_guard.as_ref().sum;
                ipc_token = rsp_guard.release();
            }
            Err(e) => {
                debug_println!("addition request call failed: {:?}", e);
                panic!("Addition requester panic'd")
            }
        }
        debug_println!("A: {}", current_sum);
    }
    debug_println!("addition_requester completed its task");
}

pub extern "C" fn addition_responder(
    params_and_ipc: (ResponderParams<role::Local>, IPCBufferToken),
) {
    let (p, ipc_token) = params_and_ipc;
    debug_println!("Inside addition_responder");
    let initial_state: usize = 0;
    p.responder
        .reply_recv_with_state(ipc_token, initial_state, move |req, state| {
            debug_println!("Addition has happened {} times", state);

            (AdditionResponse { sum: req.a + req.b },
             state + 1)
        })
        .expect("Could not set up a reply_recv");
}
