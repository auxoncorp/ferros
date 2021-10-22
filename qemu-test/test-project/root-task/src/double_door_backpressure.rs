use super::TopLevelError;

use selfe_sys::seL4_Yield;

use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, Consumer1, Consumer2, FaultOrMessage, Producer, QueueFullError,
    RetypeForSetup, Sender, StandardProcess, Waker,
};
use ferros::vspace::*;

type U66536 = Sum<U65536, U1000>;

#[ferros_test::ferros_test]
pub fn double_door_backpressure(
    local_slots: LocalCNodeSlots<U66536>,
    local_ut: LocalCap<Untyped<U27>>,
    asid_pool: LocalCap<ASIDPool<U4>>,
    local_mapped_region: MappedMemoryRegion<U19, shared_status::Exclusive>,
    local_vspace_scratch: &mut ScratchRegion,
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
        let consumer_root = retype(ut, slots)?;
        let consumer_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let consumer_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut consumer_vspace = VSpace::new(
            consumer_root,
            consumer_asid,
            consumer_vspace_slots.weaken(),
            consumer_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let producer_a_root = retype(ut, slots)?;
        let producer_a_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let producer_a_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut producer_a_vspace = VSpace::new(
            producer_a_root,
            producer_a_asid,
            producer_a_vspace_slots.weaken(),
            producer_a_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let producer_b_root = retype(ut, slots)?;
        let producer_b_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let producer_b_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut producer_b_vspace = VSpace::new(
            producer_b_root,
            producer_b_asid,
            producer_b_vspace_slots.weaken(),
            producer_b_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let waker_root = retype(ut, slots)?;
        let waker_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let waker_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut waker_vspace = VSpace::new(
            waker_root,
            waker_asid,
            waker_vspace_slots.weaken(),
            waker_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (consumer, consumer_token, producer_setup_a, waker_setup) =
            Consumer1::new::<U14, U14, _>(
                ut,
                ut,
                local_vspace_scratch,
                &mut consumer_vspace,
                &root_cnode,
                slots,
                slots,
                slots,
                slots_c,
            )?;

        let (consumer, producer_setup_b) = consumer.add_queue::<Yttrium, U14, U14, _>(
            &consumer_token,
            ut,
            local_vspace_scratch,
            &mut consumer_vspace,
            &root_cnode,
            slots,
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
        let producer_a = Producer::new(
            &producer_setup_a,
            slots_a,
            &mut producer_a_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_a_params = ProducerXParams::<role::Child> {
            producer: producer_a,
        };

        let (slots_b, _producer_b_slots) = producer_b_slots.alloc();
        let producer_b = Producer::new(
            &producer_setup_b,
            slots_b,
            &mut producer_b_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_b_params = ProducerYParams::<role::Child> {
            producer: producer_b,
        };

        let (slots_w, _waker_slots) = waker_slots.alloc();
        let waker = Waker::new(&waker_setup, slots_w, &root_cnode)?;
        let waker_params = WakerParams::<role::Child> { waker };

        let (u18_region_a, u18_region_b) = local_mapped_region.split()?;
        let (consumer_region, producer_a_region) = u18_region_a.split()?;
        let (producer_b_region, waker_region) = u18_region_b.split()?;

        let mut consumer_process = StandardProcess::new(
            &mut consumer_vspace,
            consumer_cnode,
            consumer_region,
            root_cnode,
            consumer_proc as extern "C" fn(_) -> (),
            consumer_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        let mut producer_a_process = StandardProcess::new(
            &mut producer_a_vspace,
            producer_a_cnode,
            producer_a_region,
            root_cnode,
            producer_a_proc as extern "C" fn(_) -> (),
            producer_a_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        let mut producer_b_process = StandardProcess::new(
            &mut producer_b_vspace,
            producer_b_cnode,
            producer_b_region,
            root_cnode,
            producer_b_proc as extern "C" fn(_) -> (),
            producer_b_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        let mut waker_process = StandardProcess::new(
            &mut waker_vspace,
            waker_cnode,
            waker_region,
            root_cnode,
            waker_proc as extern "C" fn(_) -> (),
            waker_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        consumer_process.start()?;
        producer_a_process.start()?;
        producer_b_process.start()?;
        waker_process.start()?;
    });

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

pub struct Xenon {
    a: u64,
    padding: [u8; 1024],
}

pub struct Yttrium {
    b: u64,
    padding: [u8; 1024],
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer2<Role, Xenon, Yttrium>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerXParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon>,
}

impl RetypeForSetup for ProducerXParams<role::Local> {
    type Output = ProducerXParams<role::Child>;
}

pub struct ProducerYParams<Role: CNodeRole> {
    pub producer: Producer<Role, Yttrium>,
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

pub extern "C" fn consumer_proc(p: ConsumerParams<role::Local>) {
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
    assert_eq!(consumer.capacity(), (U14::USIZE, U14::USIZE));
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

pub extern "C" fn waker_proc(p: WakerParams<role::Local>) {
    p.waker.send_wakeup_signal();
}

pub extern "C" fn producer_a_proc(p: ProducerXParams<role::Local>) {
    assert_eq!(p.producer.capacity(), U14::USIZE);
    assert_eq!(p.producer.is_full(), false);
    for i in 0..20 {
        let mut x = Xenon {
            a: i,
            padding: [0; 1024],
        };
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

pub extern "C" fn producer_b_proc(p: ProducerYParams<role::Local>) {
    assert_eq!(p.producer.capacity(), U14::USIZE);
    assert_eq!(p.producer.is_full(), false);
    let mut rejection_count = 0;
    for i in 0..20 {
        let mut y = Yttrium {
            b: i,
            padding: [0; 1024],
        };
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
