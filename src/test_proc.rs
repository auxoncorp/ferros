use crate::userland::{
    role, CNodeRole, Consumer2, InterruptConsumer, Producer, QueueFullError, RetypeForSetup, Waker,
};
use selfe_sys::seL4_Yield;
use typenum::{U10, U2, U58};

#[derive(Debug)]
pub struct Xenon {
    a: u64,
}

#[derive(Debug)]
pub struct Yttrium {
    b: u64,
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer2<Role, Xenon, U10, Yttrium, U2>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerYParams<Role: CNodeRole> {
    pub producer: Producer<Role, Yttrium, U2>,
    pub interrupt_consumer: InterruptConsumer<U58, Role>,
}

impl RetypeForSetup for ProducerYParams<role::Local> {
    type Output = ProducerYParams<role::Child>;
}

pub struct ProducerXParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon, U10>,
}

impl RetypeForSetup for ProducerXParams<role::Local> {
    type Output = ProducerXParams<role::Child>;
}

pub struct WakerParams<Role: CNodeRole> {
    pub waker: Waker<Role>,
}

impl RetypeForSetup for WakerParams<role::Local> {
    type Output = WakerParams<role::Child>;
}

#[allow(dead_code)]
pub extern "C" fn consumer_process(p: ConsumerParams<role::Local>) {
    #[derive(Debug)]
    struct State {
        interrupt_count: usize,
        queue_e_element_count: usize,
        queue_e_sum: u64,
        queue_f_element_count: usize,
        queue_f_sum: u64,
    }

    impl State {
        fn debug_if_finished(&self) {
            if self.interrupt_count == 1
                && self.queue_e_element_count == 20
                && self.queue_f_element_count == 20
            {
                debug_println!("Final state: {:?}", self);
            }
        }
    }

    debug_println!("Inside consumer");
    let initial_state = State {
        interrupt_count: 0,
        queue_e_element_count: 0,
        queue_e_sum: 0,
        queue_f_element_count: 0,
        queue_f_sum: 0,
    };
    p.consumer.consume(
        initial_state,
        |mut state| {
            debug_println!("Non-queue multiconsumer wakeup happened!");
            state.interrupt_count = state.interrupt_count.saturating_add(1);
            state.debug_if_finished();
            state
        },
        |x, mut state| {
            debug_println!("Pulling from Queue E, Xenon: {:?}", x);
            state.queue_e_element_count = state.queue_e_element_count.saturating_add(1);
            state.queue_e_sum = state.queue_e_sum.saturating_add(x.a);
            state.debug_if_finished();
            state
        },
        |y, mut state| {
            debug_println!("Pulling from Queue F, Yttrium: {:?}", y);
            state.queue_f_element_count = state.queue_f_element_count.saturating_add(1);
            state.queue_f_sum = state.queue_f_sum.saturating_add(y.b);
            state.debug_if_finished();
            state
        },
    )
}

#[allow(dead_code)]
pub extern "C" fn waker_process(p: WakerParams<role::Local>) {
    debug_println!("Inside waker");
    p.waker.send_wakeup_signal();
}

#[allow(dead_code)]
pub extern "C" fn producer_x_process(p: ProducerXParams<role::Local>) {
    debug_println!("Inside producer x");
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

#[allow(dead_code)]
pub extern "C" fn producer_y_process(p: ProducerYParams<role::Local>) {
    debug_println!("Inside producer y ");
    let producer = p.producer;
    #[derive(Debug)]
    struct State {
        interrupt_count: u64,
        rejection_count: u64,
    };
    p.interrupt_consumer.consume(
        State {
            interrupt_count: 0,
            rejection_count: 0,
        },
        move |mut state| {
            debug_println!(
                "Got a real interrupt notification, let's try producing to the other queue."
            );
            let mut y = Yttrium {
                b: state.interrupt_count,
            };
            loop {
                match producer.send(y) {
                    Ok(_) => {
                        break;
                    }
                    Err(QueueFullError(rejected_y)) => {
                        y = rejected_y;
                        state.rejection_count = state.interrupt_count.saturating_add(1);
                        unsafe {
                            seL4_Yield();
                        }
                    }
                }
            }
            state.interrupt_count = state.interrupt_count.saturating_add(1);
            state
        },
    );
}
