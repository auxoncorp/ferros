use core::marker::PhantomData;
use crate::micro_alloc::{self, GetUntyped};
use crate::pow::Pow;
use crate::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, spawn, BootInfo, CNode, CNodeRole,
    Caller, Cap, Endpoint, FaultSink, LocalCap, MappedPage, Responder, RetypeForSetup,
    UnmappedPageTable, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::{U12, U2, U20, U4096, U6};

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
    pub shared_page: MappedPage,
}

impl RetypeForSetup for CallerParams<role::Local> {
    type Output = CallerParams<role::Child>;
}

#[derive(Debug)]
pub struct ResponderParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U2>, Role>, Role>,
    pub responder: Responder<AdditionRequest, AdditionResponse, Role>,
    pub shared_page: MappedPage,
}

impl RetypeForSetup for ResponderParams<role::Local> {
    type Output = ResponderParams<role::Child>;
}

pub extern "C" fn caller(p: CallerParams<role::Local>) {
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

pub extern "C" fn responder(p: ResponderParams<role::Local>) {
    debug_println!("Inside addition_responder");
    let initial_state: usize = 0;
    p.responder
        .reply_recv_with_state(initial_state, move |req, state| {
            debug_println!("Addition has happened {} times", state);

            (AdditionResponse { sum: req.a + req.b }, state + 1)
        })
        .expect("Could not set up a reply_recv");
}

// #[derive(Debug)]
// pub struct CapFaulterParams<Role: CNodeRole> {
//     pub _role: PhantomData<Role>,
// }

// impl RetypeForSetup for CapFaulterParams<role::Local> {
//     type Output = CapFaulterParams<role::Child>;
// }

// #[derive(Debug)]
// pub struct VMFaulterParams<Role: CNodeRole> {
//     pub _role: PhantomData<Role>,
// }

// impl RetypeForSetup for VMFaulterParams<role::Local> {
//     type Output = VMFaulterParams<role::Child>;
// }

// #[derive(Debug)]
// pub struct MischiefDetectorParams<Role: CNodeRole> {
//     pub fault_sink: FaultSink<Role>,
// }

// impl RetypeForSetup for MischiefDetectorParams<role::Local> {
//     type Output = MischiefDetectorParams<role::Child>;
// }

// pub extern "C" fn vm_fault_source_proc(_p: VMFaulterParams<role::Local>) {
//     debug_println!("Inside vm_fault_source_proc");
//     debug_println!("Attempting to cause a segmentation fault");
//     unsafe {
//         let x: *const usize = 0x88888888usize as _;
//         let y = *x;
//         debug_println!(
//             "Value from arbitrary memory is: {}, but we shouldn't get far enough to print it",
//             y
//         );
//     }
//     debug_println!("This is after the vm fault inducing code, and should not be printed.");
// }

// pub extern "C" fn cap_fault_source_proc(_p: CapFaulterParams<role::Local>) {
//     debug_println!("Inside cap_fault_source_proc");
//     debug_println!("\nAttempting to cause a cap fault");
//     unsafe {
//         seL4_Send(
//             314159, // bogus cptr to nonexistent endpoint
//             seL4_MessageInfo_new(0, 0, 0, 0),
//         )
//     }
//     debug_println!("This is after the capability fault inducing code, and should not be printed.");
// }

// pub extern "C" fn fault_sink_proc(p: MischiefDetectorParams<role::Local>) {
//     debug_println!("Inside fault_sink_proc");
//     let mut last_sender = Badge::from(0usize);
//     for i in 1..=2 {
//         let fault = p.fault_sink.wait_for_fault();
//         debug_println!("Caught fault {}: {:?}", i, fault);
//         if last_sender == fault.sender() {
//             debug_println!("Fault badges were equal, but we wanted distinct senders");
//             panic!("Stop the presses")
//         }
//         last_sender = fault.sender();
//     }
//     debug_println!("Successfully caught 2 faults from 2 distinct sources");
// }
