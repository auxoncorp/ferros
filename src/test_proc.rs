use crate::pow::Pow;
use crate::userland::{role, CNode, CNodeRole, Caller, Cap, MappedPage, Responder, RetypeForSetup};
use typenum::operator_aliases::Diff;
use typenum::{U12, U2};

/////////////////
// Serial echo //
/////////////////
use crate::drivers::uart::basic::{UARTCommand, UARTResponse};

pub struct EchoParams<Role: CNodeRole> {
    pub uart: Caller<UARTCommand, UARTResponse, Role>,
}

impl RetypeForSetup for EchoParams<role::Local> {
    type Output = EchoParams<role::Child>;
}

fn get_byte(uart: &Caller<UARTCommand, UARTResponse, role::Local>) -> u8 {
    use self::UARTResponse::*;

    match uart.blocking_call(&UARTCommand::GetByte) {
        Ok(GotByte(b)) => b,
        Ok(Error) => panic!("getchar error"),
        Err(_) => panic!("system error"),
        _ => panic!("unexpected getchar response"),
    }
}

fn put_byte(uart: &Caller<UARTCommand, UARTResponse, role::Local>, value: u8) {
    use self::UARTResponse::*;

    match uart.blocking_call(&UARTCommand::PutByte(value)) {
        Ok(WroteByte) => (),
        Ok(Error) => panic!("putchar error"),
        Err(_) => panic!("system error"),
        _ => panic!("unexpected getchar response"),
    };
}

pub extern "C" fn echo(p: EchoParams<role::Local>) {
    debug_println!("Starting echo process");
    let uart = p.uart;

    loop {
        let b = get_byte(&uart);
        put_byte(&uart, b);
        put_byte(&uart, '!' as u8);
    }
}

///////////////////////
// Addition with shm //
///////////////////////

#[derive(Debug)]
pub struct AdditionRequest {
    a: u32,
    b: u32,
}

#[derive(Debug)]
pub struct AdditionResponse {
    sum: u32,
}

struct SharedData {
    c: u32,
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

    let shared: &mut SharedData =
        unsafe { core::mem::transmute(p.shared_page.vaddr as *mut SharedData) };

    while current_sum < 100 {
        addition_request.a = current_sum;
        addition_request.b = current_sum;
        shared.c = current_sum;

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
    let shared: &SharedData =
        unsafe { core::mem::transmute(p.shared_page.vaddr as *const SharedData) };

    p.responder
        .reply_recv_with_state(initial_state, move |req, state| {
            debug_println!("Addition has happened {} times", state);
            debug_println!("SHM says that shared.c={}", shared.c);

            (AdditionResponse { sum: req.a + req.b }, state + 1)
        })
        .expect("Could not set up a reply_recv");
}
