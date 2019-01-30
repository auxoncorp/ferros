use crate::userland::{role, CNodeRole, ExtendedCaller, ExtendedResponder, RetypeForSetup};

pub struct AdditionRequest {
    a: u64,
    b: u64,
    history: [u64; 256],
}

#[derive(Debug)]
pub struct AdditionResponse {
    sum: u64,
}

pub struct ExtendedCallerParams<Role: CNodeRole> {
    pub caller: ExtendedCaller<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for ExtendedCallerParams<role::Local> {
    type Output = ExtendedCallerParams<role::Child>;
}

pub struct ExtendedResponderParams<Role: CNodeRole> {
    pub responder: ExtendedResponder<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for ExtendedResponderParams<role::Local> {
    type Output = ExtendedResponderParams<role::Child>;
}

pub extern "C" fn caller(p: ExtendedCallerParams<role::Local>) {
    debug_println!("Inside addition_requester");
    let mut current_sum = 1;
    let mut caller = p.caller;
    let mut addition_request = AdditionRequest {
        a: current_sum,
        b: current_sum,
        history: [0; 256],
    };

    for i in 0..100 {
        addition_request.a = current_sum;
        addition_request.b = current_sum;

        debug_println!(
            "Q: What is {} + {} - ({} / 2)?",
            addition_request.a,
            addition_request.b,
            addition_request.b
        );
        let AdditionResponse { sum } = caller.blocking_call(&addition_request);
        current_sum = sum;
        addition_request.history[i] = current_sum;
        debug_println!("A: {}", current_sum);
    }
    debug_println!(
        "Call and response addition finished. Last equation: {} + {} = {}",
        addition_request.a,
        addition_request.b,
        current_sum,
    );
    debug_print!("History: [");
    for h in addition_request.history.iter().take(100) {
        debug_print!("{}, ", h);
    }
    debug_print!("]\n");
}

pub extern "C" fn responder(p: ExtendedResponderParams<role::Local>) {
    debug_println!("Inside addition_responder");
    let initial_state: usize = 0;

    p.responder
        .reply_recv_with_state(initial_state, move |req, state| {
            debug_println!("Addition has happened {} times", state);
            (
                AdditionResponse {
                    sum: req.a + req.b - (req.b / 2),
                },
                state + 1,
            )
        })
        .expect("Could not set up a reply_recv");
}
