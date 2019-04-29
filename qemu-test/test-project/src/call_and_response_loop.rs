use super::TopLevelError;
use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::userland::{
    call_channel, retype, retype_cnode, role, root_cnode, BootInfo, CNodeRole, Caller, Consumer1,
    Producer, Responder, RetypeForSetup, VSpace,
};
use selfe_sys::*;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");
    let uts = alloc::ut_buddy(ut20);

    smart_alloc!(|slots from local_slots, ut from uts| {
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        let scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

        let (child_a_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (child_b_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
    });

    let (
        child_params_a,
        proc_cnode_local_a,
        child_fault_source_a,
        child_params_b,
        proc_cnode_local_b,
        child_fault_source_b,
        local_slots,
        uts,
    ) = {
        smart_alloc!(|slots from local_slots, ut from uts| {
            let (caller_cnode, caller_slots) = retype_cnode::<U12>(ut, slots)?;
            let (responder_cnode, responder_slots) = retype_cnode::<U12>(ut, slots)?;

            let (slots_r, _responder_slots) = responder_slots.alloc();
            let (ipc_setup, responder) = call_channel(ut, &root_cnode, slots, slots_r)?;

            let (slots_c, _caller_slots) = caller_slots.alloc();
            let caller = ipc_setup.create_caller(slots_c)?;

            let caller_params = CallerParams::<role::Child> {
                caller,
            };

            let responder_params = ResponderParams::<role::Child> {
                responder,
            };
        });

        (
            caller_params,
            caller_cnode,
            None,
            responder_params,
            responder_cnode,
            None,
            local_slots,
            uts,
        )
    };

    smart_alloc!(|slots from local_slots, ut from uts| {
        let (child_a_process, _) = child_a_vspace.prepare_thread(
            child_proc_a,
            child_params_a,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;
        child_a_process.start(
            proc_cnode_local_a,
            child_fault_source_a,
            &boot_info.tcb,
            255,
        )?;

        let (child_b_process, _) = child_b_vspace.prepare_thread(
            child_proc_b,
            child_params_b,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;
        child_b_process.start(
            proc_cnode_local_b,
            child_fault_source_b,
            &boot_info.tcb,
            255,
        )?;
    });

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
    pub caller: Caller<AdditionRequest, AdditionResponse, Role>,
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
