use crate::userland::{role, CNodeRole, Consumer1, Producer, RetypeForSetup};
use cross_queue::PushError;
use sel4_sys::seL4_Yield;
use typenum::U100;

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

pub extern "C" fn consumer(p: ConsumerParams<role::Local>) {
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

pub extern "C" fn producer(p: ProducerParams<role::Local>) {
    debug_println!("Inside producer");
    for i in 0..256 {
        match p.producer.send(Xenon { a: i }) {
            Ok(_) => {
                debug_println!("The producer *thinks* it successfully sent {}", i);
            }
            Err(PushError(x)) => {
                debug_println!("Rejected sending {:?}", x);
                unsafe {
                    seL4_Yield();
                }
            }
        }
    }
}
