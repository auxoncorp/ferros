use core::marker::PhantomData;
use iron_pegasus::micro_alloc::{self, GetUntyped};
use iron_pegasus::pow::Pow;
use iron_pegasus::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, spawn, BootInfo, CNode, CNodeRole,
    Caller, Cap, Endpoint, FaultSink, LocalCap, Responder, RetypeForSetup, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::{U12, U2, U20, U4096, U6};

pub fn run(raw_boot_info: &'static seL4_BootInfo) {
    // wrap all untyped memory
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)
        .expect("Couldn't set up bootstrap allocator");

    // wrap root CNode for safe usage
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode).expect("quarter");
    let (ut16a, ut16b, ut16c, ut16d, root_cnode) = ut18.quarter(root_cnode).expect("quarter");
    let (ut16e, _, _, _, root_cnode) = ut18b.quarter(root_cnode).expect("quarter");
    let (ut14, _, _, _, root_cnode) = ut16e.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, _, _, root_cnode) = ut14.quarter(root_cnode).expect("quarter");
    let (ut10, _, _, _, root_cnode) = ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");
    let (ut5, _, root_cnode) = ut6.split(root_cnode).expect("split");
    let (ut4, _, root_cnode) = ut5.split(root_cnode).expect("split"); // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    #[cfg(test_case = "call_and_response_loop")]
    let (child_params_a, proc_cnode_local_a, child_params_b, proc_cnode_local_b, root_cnode) = {
        let (caller_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16a
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to caller_cnode_local");

        let (responder_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to responder_cnode_local");

        let (caller_cnode_local, responder_cnode_local, caller, responder, root_cnode) =
            call_channel(root_cnode, ut4, caller_cnode_local, responder_cnode_local)
                .expect("Could not make fastpath call channel");

        let (caller_cnode_child, caller_cnode_local) = caller_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("caller self awareness");
        let (responder_cnode_child, responder_cnode_local) = responder_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("responder self awareness");

        let caller_params = CallerParams::<role::Child> {
            my_cnode: caller_cnode_child,
            caller,
        };

        let responder_params = ResponderParams::<role::Child> {
            my_cnode: responder_cnode_child,
            responder,
        };
        (
            caller_params,
            caller_cnode_local,
            responder_params,
            responder_cnode_local,
            root_cnode,
        )
    };
    #[cfg(test_case = "fault_pair")]
    let (child_params_a, proc_cnode_local_a, child_params_b, proc_cnode_local_b, root_cnode) = {
        let (fault_source_cnode_local, root_cnode): (
            LocalCap<CNode<U4096, role::Child>>,
            _,
        ) = ut16a
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to caller_cnode_local");

        let (fault_sink_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to responder_cnode_local");

        let (
            fault_source_cnode_local,
            fault_sink_cnode_local,
            fault_source,
            fault_sink,
            root_cnode,
        ) = setup_fault_endpoint_pair(
            root_cnode,
            ut4,
            fault_source_cnode_local,
            fault_sink_cnode_local,
        )
        .expect("Could not make a fault endpoint pair");

        // self-reference must come last because it seals our ability to add more capabilities
        // from the current thread's perspective
        let (_caller_cnode_child, caller_cnode_local) = fault_source_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("caller self awareness");
        let (_responder_cnode_child, responder_cnode_local) = fault_sink_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("responder self awareness");

        let caller_params = MischiefMakerParams { _role: PhantomData };

        let responder_params = MischiefDetectorParams::<role::Child> { fault_sink };
        (
            caller_params,
            caller_cnode_local,
            responder_params,
            responder_cnode_local,
            root_cnode,
        )
    };

    let root_cnode = spawn(
        child_proc_a,
        child_params_a,
        proc_cnode_local_a,
        255,  // priority
        None, // fault_source
        ut16c,
        &mut boot_info,
        root_cnode,
    )
    .expect("spawn process 2");

    let root_cnode = spawn(
        child_proc_b,
        child_params_b,
        proc_cnode_local_b,
        255,  // priority
        None, // fault_source
        ut16d,
        &mut boot_info,
        root_cnode,
    )
    .expect("spawn process 2");
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
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub caller: Caller<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for CallerParams<role::Local> {
    type Output = CallerParams<role::Child>;
}

#[derive(Debug)]
pub struct ResponderParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub responder: Responder<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for ResponderParams<role::Local> {
    type Output = ResponderParams<role::Child>;
}

#[cfg(test_case = "call_and_response_loop")]
pub extern "C" fn child_proc_a(p: CallerParams<role::Local>) {
    debug_println!("Inside addition_requester");
    let mut current_sum: u32 = 1;
    let caller = p.caller;
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
        match caller.blocking_call(&addition_request) {
            Ok(rsp) => {
                current_sum = rsp.sum;
            }
            Err(e) => {
                debug_println!("addition request call failed: {:?}", e);
                panic!("Addition requester panic'd")
            }
        }
        debug_println!("A: {}", current_sum);
    }
    debug_println!(
        "Call and response addition finished. Last equation: {} + {} = {}",
        addition_request.a,
        addition_request.b,
        current_sum
    );
}

#[cfg(test_case = "call_and_response_loop")]
pub extern "C" fn child_proc_b(p: ResponderParams<role::Local>) {
    debug_println!("Inside addition_responder");
    let initial_state: usize = 0;
    p.responder
        .reply_recv_with_state(initial_state, move |req, state| {
            debug_println!("Addition has happened {} times", state);

            (AdditionResponse { sum: req.a + req.b }, state + 1)
        })
        .expect("Could not set up a reply_recv");
}

#[derive(Debug)]
pub struct MischiefMakerParams<Role: CNodeRole> {
    pub _role: PhantomData<Role>,
}

impl RetypeForSetup for MischiefMakerParams<role::Local> {
    type Output = MischiefMakerParams<role::Child>;
}

#[derive(Debug)]
pub struct MischiefDetectorParams<Role: CNodeRole> {
    pub fault_sink: FaultSink<Role>,
}

impl RetypeForSetup for MischiefDetectorParams<role::Local> {
    type Output = MischiefDetectorParams<role::Child>;
}

#[cfg(test_case = "fault_pair")]
pub extern "C" fn child_proc_a(_p: MischiefMakerParams<role::Local>) {
    debug_println!("Inside fault_source_proc");
    debug_println!("\nAttempting to cause a cap fault");
    unsafe {
        seL4_Send(
            314159, // bogus cptr to nonexistent endpoint
            seL4_MessageInfo_new(0, 0, 0, 0),
        )
    }
    debug_println!("This is after the capability fault inducing code, and should not be printed.");
}

#[cfg(test_case = "fault_pair")]
pub extern "C" fn child_proc_b(p: MischiefDetectorParams<role::Local>) {
    debug_println!("Inside fault_sink_proc");
    let fault = p.fault_sink.wait_for_fault();
    debug_println!("Caught a fault: {:?}", fault);
}
