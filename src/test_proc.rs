use crate::userland::{
    role, CNodeRole, Consumer1, Producer, QueueFullError, RetypeForSetup, Waker,
};
use sel4_sys::seL4_Yield;
use typenum::U10;

#[derive(Debug)]
pub struct Xenon {
    a: u64,
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer1<Role, Xenon, U10>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon, U10>,
}

impl RetypeForSetup for ProducerParams<role::Local> {
    type Output = ProducerParams<role::Child>;
}

pub struct WakerParams<Role: CNodeRole> {
    pub waker: Waker<Role>,
}

impl RetypeForSetup for WakerParams<role::Local> {
    type Output = WakerParams<role::Child>;
}

pub extern "C" fn consumer(p: ConsumerParams<role::Local>) {
    #[derive(Debug)]
    struct State {
        interrupt_count: usize,
        element_count: usize,
        queue_sum: u64,
    }

    debug_println!("Inside consumer");
    let initial_state = State {
        interrupt_count: 0,
        element_count: 0,
        queue_sum: 0,
    };
    p.consumer.consume(
        initial_state,
        |state| {
            let fresh_state = State {
                interrupt_count: state.interrupt_count.saturating_add(1),
                element_count: state.element_count,
                queue_sum: state.queue_sum,
            };
            if fresh_state.element_count == 40 && fresh_state.interrupt_count == 1 {
                debug_println!(
                    "Creating fresh state {:?} in the waker callback",
                    fresh_state
                );
            }
            fresh_state
        },
        |x, state| {
            let fresh_state = State {
                interrupt_count: state.interrupt_count,
                element_count: state.element_count.saturating_add(1),
                queue_sum: state.queue_sum.saturating_add(x.a),
            };
            if fresh_state.element_count == 40 && fresh_state.interrupt_count == 1 {
                debug_println!(
                    "Creating fresh state {:?} in the queue callback",
                    fresh_state
                );
            }
            fresh_state
        },
    )
}

pub extern "C" fn waker(p: WakerParams<role::Local>) {
    debug_println!("Inside waker");
    p.waker.send_wakeup_signal();
}

pub extern "C" fn producer(p: ProducerParams<role::Local>) {
    debug_println!("Inside producer");
    let mut rejection_count = 0;
    for i in 0..20 {
        let mut x = Xenon { a: i };
        loop {
            match p.producer.send(x) {
                Ok(_) => {
                    break;
                }
                Err(QueueFullError(rejected_x)) => {
                    x = rejected_x;
                    rejection_count += 1;
                    unsafe {
                        seL4_Yield();
                    }
                }
            }
        }
    }
    debug_println!("\n\nProducer rejection count: {}\n\n", rejection_count);
}
