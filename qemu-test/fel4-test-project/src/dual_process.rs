use core::marker::PhantomData;
use ferros::micro_alloc::{self, GetUntyped};
use ferros::pow::Pow;
use ferros::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, BootInfo, CNode,
    CNodeRole, Caller, Cap, Consumer1, Endpoint, FaultSink, LocalCap, Producer, ProducerSetup,
    QueueFullError, Responder, RetypeForSetup, SeL4Error, UnmappedPageTable, Untyped, VSpace,
};
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::{U100, U12, U2, U20, U4096, U6};

use sel4_sys::seL4_Yield;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), SeL4Error> {
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
    let (ut16a, ut16b, child_a_vspace_ut, child_b_vspace_ut, root_cnode) =
        ut18.quarter(root_cnode).expect("quarter");
    let (ut16e, _, _, _, root_cnode) = ut18b.quarter(root_cnode).expect("quarter");
    let (ut14, child_a_thread_ut, child_b_thread_ut, _, root_cnode) =
        ut16e.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) =
        ut14.quarter(root_cnode).expect("quarter");
    let (ut10, scratch_page_table_ut, _, _, root_cnode) =
        ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");
    let (ut5, _, root_cnode) = ut6.split(root_cnode).expect("split");
    let (ut4, _, root_cnode) = ut5.split(root_cnode).expect("split"); // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) = scratch_page_table_ut
        .retype_local(root_cnode)
        .expect("retype scratch page table");
    let (mut scratch_page_table, mut boot_info) = boot_info
        .map_page_table(scratch_page_table)
        .expect("map scratch page table");

    let (child_a_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, child_a_vspace_ut, root_cnode)?;
    let (child_b_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, child_b_vspace_ut, root_cnode)?;

    #[cfg(test_case = "shared_page_queue")]
    let (
        child_params_a,
        proc_cnode_local_a,
        child_a_vspace,
        child_fault_source_a,
        child_params_b,
        proc_cnode_local_b,
        child_b_vspace,
        child_fault_source_b,
        root_cnode,
    ) = {
        let (producer_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16a
            .retype_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to producer_cnode_local");

        let (consumer_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to consumer_cnode_local");

        let (consumer, producer_setup, waker_setup, consumer_cnode, consumer_vspace, root_cnode) =
            Consumer1::new(
                shared_page_ut,
                ut4,
                consumer_cnode_local,
                child_a_vspace,
                &mut scratch_page_table,
                &mut boot_info.page_directory,
                root_cnode,
            )
            .expect("ipc error");

        let consumer_params = ConsumerParams::<role::Child> { consumer };

        let (producer, producer_cnode, producer_vspace, root_cnode) = Producer::new(
            &producer_setup,
            producer_cnode_local,
            child_b_vspace,
            root_cnode,
        )?;

        let producer_params = ProducerParams::<role::Child> { producer };
        (
            consumer_params,
            consumer_cnode,
            consumer_vspace,
            None,
            producer_params,
            producer_cnode,
            producer_vspace,
            None,
            root_cnode,
        )
    };

    #[cfg(test_case = "call_and_response_loop")]
    let (
        child_params_a,
        proc_cnode_local_a,
        child_fault_source_a,
        child_params_b,
        proc_cnode_local_b,
        child_fault_source_b,
        root_cnode,
    ) = {
        let (caller_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16a
            .retype_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to caller_cnode_local");

        let (responder_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_cnode::<_, U12>(root_cnode)
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
            None,
            responder_params,
            responder_cnode_local,
            None,
            root_cnode,
        )
    };
    #[cfg(test_case = "fault_pair")]
    let (
        child_params_a,
        proc_cnode_local_a,
        child_fault_source_a,
        child_params_b,
        proc_cnode_local_b,
        child_fault_source_b,
        root_cnode,
    ) = {
        let (fault_source_cnode_local, root_cnode): (
            LocalCap<CNode<U4096, role::Child>>,
            _,
        ) = ut16a
            .retype_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to caller_cnode_local");

        let (fault_sink_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_cnode::<_, U12>(root_cnode)
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
            None,
            responder_params,
            responder_cnode_local,
            Some(fault_source),
            root_cnode,
        )
    };
    let (child_a_process, _caller_vspace, root_cnode) = child_a_vspace
        .prepare_thread(
            child_proc_a,
            child_params_a,
            child_a_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare thread child_a");
    child_a_process.start(
        proc_cnode_local_a,
        child_fault_source_a,
        &boot_info.tcb,
        255,
    )?;

    let (child_b_process, _caller_vspace, root_cnode) = child_b_vspace
        .prepare_thread(
            child_proc_b,
            child_params_b,
            child_b_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare thread child_b");
    child_b_process.start(
        proc_cnode_local_b,
        child_fault_source_b,
        &boot_info.tcb,
        255,
    )?;

    Ok(())
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

#[derive(Debug)]
pub struct Xenon {
    a: u64,
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer1<Role, Xenon, U100>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon, U100>,
}

impl RetypeForSetup for ProducerParams<role::Local> {
    type Output = ProducerParams<role::Child>;
}

#[cfg(test_case = "shared_page_queue")]
pub extern "C" fn child_proc_a(p: ConsumerParams<role::Local>) {
    debug_println!("Inside consumer");
    let initial_state = 0;
    p.consumer.consume(
        initial_state,
        |state| {
            let fresh_state = state + 1;
            debug_println!("Creating fresh state {} in the waker callback", fresh_state);
            fresh_state
        },
        |x, state| {
            let fresh_state = x.a + state;
            debug_println!(
                "Creating fresh state {} from {:?} and {} in the queue callback",
                fresh_state,
                x,
                state
            );
            fresh_state
        },
    )
}

#[cfg(test_case = "shared_page_queue")]
pub extern "C" fn child_proc_b(p: ProducerParams<role::Local>) {
    debug_println!("Inside producer");
    for i in 0..256 {
        match p.producer.send(Xenon { a: i }) {
            Ok(_) => {
                debug_println!("The producer *thinks* it successfully sent {}", i);
            }
            Err(QueueFullError(x)) => {
                debug_println!("Rejected sending {:?}", x);
                unsafe {
                    seL4_Yield();
                }
            }
        }
        debug_println!("done producing!");
    }
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
