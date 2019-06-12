use super::TopLevelError;

use selfe_sys::seL4_Yield;

use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, Consumer1, Consumer2, FaultOrMessage, Producer, QueueFullError,
    RetypeForSetup, Sender, Waker,
};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

type U66536 = Sum<U65536, U1000>;

#[ferros_test::ferros_test]
pub fn double_door_backpressure(
    local_slots: LocalCNodeSlots<U66536>,
    local_ut: LocalCap<Untyped<U27>>,
    asid_pool: LocalCap<ASIDPool<U4>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (consumer_asid, asid_pool) = asid_pool.alloc();
        let (producer_a_asid, asid_pool) = asid_pool.alloc();
        let (producer_b_asid, asid_pool) = asid_pool.alloc();
        let (waker_asid, _asid_pool) = asid_pool.alloc();

        let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_a_cnode, producer_a_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_b_cnode, producer_b_slots) = retype_cnode::<U12>(ut, slots)?;
        let (waker_cnode, waker_slots) = retype_cnode::<U12>(ut, slots)?;

        // vspace setup
        let consumer_vspace = VSpace::new(ut, slots, consumer_asid, &user_image, &root_cnode)?;
        let producer_a_vspace = VSpace::new(ut, slots, producer_a_asid, &user_image, &root_cnode)?;
        let producer_b_vspace = VSpace::new(ut, slots, producer_b_asid, &user_image, &root_cnode)?;
        let waker_vspace = VSpace::new(ut, slots, waker_asid, &user_image, &root_cnode)?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (consumer, consumer_token, producer_setup_a, waker_setup, consumer_vspace) =
            Consumer1::new(
                ut,
                ut,
                consumer_vspace,
                local_vspace_scratch,
                &root_cnode,
                slots,
                slots_c,
            )?;

        let (consumer, producer_setup_b, consumer_vspace) = consumer.add_queue(
            &consumer_token,
            ut,
            consumer_vspace,
            local_vspace_scratch,
            &root_cnode,
            slots,
        )?;
        let (outcome_sender_slots, _consumer_slots) = consumer_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, outcome_sender_slots, slots)?;

        let consumer_params = ConsumerParams::<role::Child> {
            consumer,
            outcome_sender,
        };

        let (slots_a, _producer_a_slots) = producer_a_slots.alloc();
        let (producer_a, producer_a_vspace) = Producer::new(
            &producer_setup_a,
            slots_a,
            producer_a_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_a_params = ProducerXParams::<role::Child> {
            producer: producer_a,
        };

        let (slots_b, _producer_b_slots) = producer_b_slots.alloc();
        let (producer_b, producer_b_vspace) = Producer::new(
            &producer_setup_b,
            slots_b,
            producer_b_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_b_params = ProducerYParams::<role::Child> {
            producer: producer_b,
        };

        let (slots_w, _waker_slots) = waker_slots.alloc();
        let waker = Waker::new(&waker_setup, slots_w, &root_cnode)?;
        let waker_params = WakerParams::<role::Child> { waker };

        let (consumer_thread, _) = consumer_vspace.prepare_thread(
            consumer_process,
            consumer_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;

        let (producer_a_thread, _) = producer_a_vspace.prepare_thread(
            producer_x_process,
            producer_a_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;

        let (producer_b_thread, _) = producer_b_vspace.prepare_thread(
            producer_y_process,
            producer_b_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;

        let (waker_thread, _) = waker_vspace.prepare_thread(
            waker_process,
            waker_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;

        consumer_thread.start(consumer_cnode, Some(fault_source), tpa, 255)?;
        producer_a_thread.start(producer_a_cnode, None, tpa, 255)?;
        producer_b_thread.start(producer_b_cnode, None, tpa, 255)?;
        waker_thread.start(waker_cnode, None, tpa, 255)?;
    });

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

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
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerXParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon, U10>,
}

impl RetypeForSetup for ProducerXParams<role::Local> {
    type Output = ProducerXParams<role::Child>;
}

pub struct ProducerYParams<Role: CNodeRole> {
    pub producer: Producer<Role, Yttrium, U2>,
}

impl RetypeForSetup for ProducerYParams<role::Local> {
    type Output = ProducerYParams<role::Child>;
}

pub struct WakerParams<Role: CNodeRole> {
    pub waker: Waker<Role>,
}

impl RetypeForSetup for WakerParams<role::Local> {
    type Output = WakerParams<role::Child>;
}

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
        fn is_finished(&self) -> bool {
            self.interrupt_count == 1
                && self.queue_e_element_count == 20
                && self.queue_f_element_count == 20
        }
    }
    let ConsumerParams {
        consumer,
        outcome_sender,
    } = p;
    let initial_state = State {
        interrupt_count: 0,
        queue_e_element_count: 0,
        queue_e_sum: 0,
        queue_f_element_count: 0,
        queue_f_sum: 0,
    };
    consumer.consume(
        initial_state,
        |mut state| {
            state.interrupt_count = state.interrupt_count.saturating_add(1);
            if state.is_finished() {
                outcome_sender
                    .blocking_send(&true)
                    .expect("Could not send final test result")
            }
            state
        },
        |x, mut state| {
            state.queue_e_element_count = state.queue_e_element_count.saturating_add(1);
            state.queue_e_sum = state.queue_e_sum.saturating_add(x.a);
            if state.is_finished() {
                outcome_sender
                    .blocking_send(&true)
                    .expect("Could not send final test result")
            }
            state
        },
        |y, mut state| {
            state.queue_f_element_count = state.queue_f_element_count.saturating_add(1);
            state.queue_f_sum = state.queue_f_sum.saturating_add(y.b);
            if state.is_finished() {
                outcome_sender
                    .blocking_send(&true)
                    .expect("Could not send final test result")
            }
            state
        },
    )
}

pub extern "C" fn waker_process(p: WakerParams<role::Local>) {
    p.waker.send_wakeup_signal();
}

pub extern "C" fn producer_x_process(p: ProducerXParams<role::Local>) {
    for i in 0..20 {
        let mut x = Xenon { a: i };
        loop {
            match p.producer.send(x) {
                Ok(_) => {
                    break;
                }
                Err(QueueFullError(rejected_x)) => {
                    x = rejected_x;
                    unsafe {
                        seL4_Yield();
                    }
                }
            }
        }
    }
}

pub extern "C" fn producer_y_process(p: ProducerYParams<role::Local>) {
    let mut rejection_count = 0;
    for i in 0..20 {
        let mut y = Yttrium { b: i };
        loop {
            match p.producer.send(y) {
                Ok(_) => {
                    break;
                }
                Err(QueueFullError(rejected_y)) => {
                    y = rejected_y;
                    rejection_count += 1;
                    unsafe {
                        seL4_Yield();
                    }
                }
            }
        }
    }
}
