use super::TopLevelError;
use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::{
    retype, retype_cnode, role, ASIDPool, Badge, CNodeRole, Cap, LocalCNode, LocalCNodeSlots,
    LocalCap, Notification, ThreadPriorityAuthority, Untyped,
};
use ferros::userland::*;
use ferros::vspace::*;
use typenum::*;

type U33768 = op!(U32768 + U1000);

#[ferros_test::ferros_test]
pub fn call_and_response_loop(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U2>>,
    local_mapped_region: MappedMemoryRegion<U18, shared_status::Exclusive>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (caller_asid, asid_pool) = asid_pool.alloc();
        let (responder_asid, _asid_pool) = asid_pool.alloc();
        let caller_root = retype(ut, slots)?;
        let caller_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let caller_vspace_ut: LocalCap<Untyped<U15>> = ut;

        let mut caller_vspace = VSpace::new(
            caller_root,
            caller_asid,
            caller_vspace_slots.weaken(),
            caller_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let responder_root = retype(ut, slots)?;
        let responder_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let responder_vspace_ut: LocalCap<Untyped<U15>> = ut;

        let mut responder_vspace = VSpace::new(
            responder_root,
            responder_asid,
            responder_vspace_slots.weaken(),
            responder_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let (caller_cnode, caller_slots) = retype_cnode::<U12>(ut, slots)?;
        let (responder_cnode, responder_slots) = retype_cnode::<U12>(ut, slots)?;
        let (slots_r, _responder_slots) = responder_slots.alloc();
        let (ipc_setup, responder) = call_channel(ut, &root_cnode, slots, slots_r)?;

        let (slots_c, caller_slots) = caller_slots.alloc();
        let caller = ipc_setup.create_caller(slots_c)?;
        let (child_fault_source_slot, caller_slots) = caller_slots.alloc();
        let (fault_source, fault_or_outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;

        let notification: LocalCap<Notification> = retype(ut, slots)?;
        let notification_badge = Badge::from(0b100);
        let (slots_c, _caller_slots) = caller_slots.alloc();
        let caller_notification =
            notification.mint(root_cnode, slots_c, CapRights::RWG, notification_badge)?;

        let caller_params = CallerParams::<role::Child> {
            caller,
            notification: caller_notification,
            outcome_sender: fault_or_outcome_sender,
        };

        let responder_params = ResponderParams::<role::Child> { responder };

        let (caller_region, responder_region) = local_mapped_region.split()?;

        let mut caller_process = StandardProcess::new(
            &mut caller_vspace,
            caller_cnode,
            caller_region,
            root_cnode,
            caller_proc as extern "C" fn(_) -> (),
            caller_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;
        caller_process.start()?;

        let mut responder_process = StandardProcess::new(
            &mut responder_vspace,
            responder_cnode,
            responder_region,
            &root_cnode,
            responder_proc as extern "C" fn(_) -> (),
            responder_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        responder_process.bind_notification(&notification)?;
        responder_process.start()?;
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
    pub notification: Cap<Notification, Role>,
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

pub extern "C" fn caller_proc(p: CallerParams<role::Local>) {
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
            Err(_) => panic!("Addition requester panic'd"),
        }

        p.notification.signal();
    }

    p.outcome_sender
        .blocking_send(
            &(current_sum == 128 && addition_request.a + addition_request.b == current_sum),
        )
        .expect("could not send outcome");
}

pub extern "C" fn responder_proc(p: ResponderParams<role::Local>) {
    p.responder
        .recv_reply_once(|req| AdditionResponse { sum: req.a + req.b })
        .expect("recv_reply_once");

    let initial_state: usize = 1;

    p.responder
        .reply_recv_with_notification(
            initial_state,
            move |req, state| (AdditionResponse { sum: req.a + req.b }, state + 1),
            move |notification_badge, state| {
                assert!(notification_badge == 0b100);
                state + 1
            },
        )
        .expect("Could not set up a reply_recv");
}
