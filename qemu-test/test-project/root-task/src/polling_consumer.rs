use super::TopLevelError;

use selfe_sys::seL4_Yield;

use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, Consumer1, FaultOrMessage, Producer, QueueFullError, RetypeForSetup,
    Sender, StandardProcess,
};
use ferros::vspace::*;

type U66536 = Sum<U65536, U1000>;

#[ferros_test::ferros_test]
pub fn polling_consumer(
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
        let (producer_asid, asid_pool) = asid_pool.alloc();

        let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_cnode, producer_slots) = retype_cnode::<U12>(ut, slots)?;

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

        let producer_root = retype(ut, slots)?;
        let producer_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let producer_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut producer_vspace = VSpace::new(
            producer_root,
            producer_asid,
            producer_vspace_slots.weaken(),
            producer_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (consumer, consumer_token, producer_setup, _waker_setup) = Consumer1::new::<U20, U12, _>(
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

        let (outcome_sender_slots, _consumer_slots) = consumer_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, outcome_sender_slots, slots)?;

        let consumer_params = ConsumerParams::<role::Child> {
            consumer,
            outcome_sender,
        };

        let (slots_p, _producer_a_slots) = producer_slots.alloc();
        let producer = Producer::new(
            &producer_setup,
            slots_p,
            &mut producer_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_params = ProducerParams::<role::Child> { producer };

        let (u18_region_a, _u18_region_b) = local_mapped_region.split()?;
        let (consumer_region, producer_region) = u18_region_a.split()?;

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

        let mut producer_process = StandardProcess::new(
            &mut producer_vspace,
            producer_cnode,
            producer_region,
            root_cnode,
            producer_proc as extern "C" fn(_) -> (),
            producer_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;

        consumer_process.start()?;
        producer_process.start()?;
    });

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

#[derive(Debug)]
pub struct Data {
    a: u64,
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer1<Role, Data>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerParams<Role: CNodeRole> {
    pub producer: Producer<Role, Data>,
}

impl RetypeForSetup for ProducerParams<role::Local> {
    type Output = ProducerParams<role::Child>;
}

pub extern "C" fn consumer_proc(p: ConsumerParams<role::Local>) {
    #[derive(Debug)]
    struct State {
        queue_element_count: usize,
        queue_sum: u64,
    }

    impl State {
        fn is_finished(&self) -> bool {
            self.queue_element_count == 20 && self.queue_sum == 190
        }
    }

    let ConsumerParams {
        mut consumer,
        outcome_sender,
    } = p;

    let mut state = State {
        queue_element_count: 0,
        queue_sum: 0,
    };

    loop {
        if let Some(data) = consumer.poll() {
            state.queue_element_count = state.queue_element_count.saturating_add(1);
            state.queue_sum = state.queue_sum.saturating_add(data.a);

            if state.is_finished() {
                outcome_sender
                    .blocking_send(&true)
                    .expect("Could not send final test result")
            }
        }

        unsafe {
            seL4_Yield();
        }
    }
}

pub extern "C" fn producer_proc(p: ProducerParams<role::Local>) {
    for i in 0..20 {
        let mut data = Data { a: i };
        loop {
            match p.producer.send(data) {
                Ok(_) => {
                    break;
                }
                Err(QueueFullError(rejected_data)) => {
                    data = rejected_data;
                    unsafe {
                        seL4_Yield();
                    }
                }
            }
        }
    }
}
