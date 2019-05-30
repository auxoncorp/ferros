use super::TopLevelError;
use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::{
    retype_cnode, role, ASIDPool, CNodeRole, LocalCNode, LocalCNodeSlots, LocalCap,
    ThreadPriorityAuthority, Untyped,
};
use ferros::userland::*;
use ferros::vspace::{VSpace, VSpaceScratchSlice};
use ferros_test::ferros_test;
use typenum::Sum;
use typenum::*;

type U33768 = Sum<U32768, U1000>;

#[ferros_test]
pub fn call_and_response_loop(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U2>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_a_asid, asid_pool) = asid_pool.alloc();
        let (child_b_asid, _asid_pool) = asid_pool.alloc();
        let child_a_vspace = VSpace::new(ut, slots, child_a_asid, &user_image, &root_cnode)?;
        let child_b_vspace = VSpace::new(ut, slots, child_b_asid, &user_image, &root_cnode)?;
        let (caller_cnode, caller_slots) = retype_cnode::<U12>(ut, slots)?;
        let (responder_cnode, responder_slots) = retype_cnode::<U12>(ut, slots)?;
        let (slots_r, _responder_slots) = responder_slots.alloc();
        let (ipc_setup, responder) = call_channel(ut, &root_cnode, slots, slots_r)?;

        let (slots_c, caller_slots) = caller_slots.alloc();
        let caller = ipc_setup.create_caller(slots_c)?;
        let (child_fault_source_slot, _caller_slots) = caller_slots.alloc();
        let (fault_source, fault_or_outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;
        let caller_params = CallerParams::<role::Child> {
            caller,
            outcome_sender: fault_or_outcome_sender,
        };

        let responder_params = ResponderParams::<role::Child> { responder };

        let (child_a_process, _) = child_a_vspace.prepare_thread(
            child_proc_a,
            caller_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;
        child_a_process.start(caller_cnode, Some(fault_source), tpa, 255)?;

        let (child_b_process, _) = child_b_vspace.prepare_thread(
            child_proc_b,
            responder_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;
        child_b_process.start(responder_cnode, None, tpa, 255)?;
    });

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

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
    pub caller: Caller<AdditionRequest, AdditionResponse, Role>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for CallerParams<role::Local> {
    type Output = CallerParams<role::Child>;
}

#[derive(Debug)]
pub struct ResponderParams<Role: CNodeRole> {
    pub responder: Responder<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for ResponderParams<role::Local> {
    type Output = ResponderParams<role::Child>;
}

pub extern "C" fn child_proc_a(p: CallerParams<role::Local>) {
    let mut current_sum: u32 = 1;
    let caller = p.caller;
    let mut addition_request = AdditionRequest {
        a: current_sum,
        b: current_sum,
    };
    while current_sum < 100 {
        addition_request.a = current_sum;
        addition_request.b = current_sum;
        match caller.blocking_call(&addition_request) {
            Ok(rsp) => {
                current_sum = rsp.sum;
            }
            Err(e) => panic!("Addition requester panic'd"),
        }
    }
    p.outcome_sender
        .blocking_send(
            &(current_sum == 128 && addition_request.a + addition_request.b == current_sum),
        )
        .expect("could not send outcome");
}

pub extern "C" fn child_proc_b(p: ResponderParams<role::Local>) {
    let initial_state: usize = 0;
    p.responder
        .reply_recv_with_state(initial_state, move |req, state| {
            (AdditionResponse { sum: req.a + req.b }, state + 1)
        })
        .expect("Could not set up a reply_recv");
}
