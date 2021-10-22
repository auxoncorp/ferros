use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, Consumer1, FaultOrMessage, Producer, QueueFullError, RetypeForSetup,
    Sender, StandardProcess,
};
use ferros::vspace::*;

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn shared_page_queue(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U2>>,
    local_mapped_region: MappedMemoryRegion<U18, shared_status::Exclusive>,
    local_vspace_scratch: &mut ScratchRegion,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_a_asid, asid_pool) = asid_pool.alloc();
        let (child_b_asid, _asid_pool) = asid_pool.alloc();

        let consumer_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let consumer_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut consumer_vspace = VSpace::new(
            retype(ut, slots)?,
            child_a_asid,
            consumer_vspace_slots.weaken(),
            consumer_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;
        let producer_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let producer_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut producer_vspace = VSpace::new(
            retype(ut, slots)?,
            child_b_asid,
            producer_vspace_slots.weaken(),
            producer_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_cnode, producer_slots) = retype_cnode::<U12>(ut, slots)?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (consumer, _consumer_token, producer_setup, _waker_setup) =
            Consumer1::new::<U100, U12, _>(
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
        let (consumer_sender_slot, _consumer_slots) = consumer_slots.alloc();
        let (consumer_fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, consumer_sender_slot, slots)?;

        let consumer_params = ConsumerParams::<role::Child> {
            consumer,
            outcome_sender,
        };

        let (slots_p, _producer_slots) = producer_slots.alloc();
        let producer = Producer::new(
            &producer_setup,
            slots_p,
            &mut producer_vspace,
            &root_cnode,
            slots,
        )?;

        let producer_params = ProducerParams::<role::Child> { producer };

        let (producer_region, consumer_region) = local_mapped_region.split()?;

        let mut consumer_process = StandardProcess::new(
            &mut consumer_vspace,
            consumer_cnode,
            consumer_region,
            root_cnode,
            consumer_run as extern "C" fn(_) -> (),
            consumer_params,
            ut,
            ut,
            slots,
            tpa,
            Some(consumer_fault_source),
        )?;
        consumer_process.start()?;

        let mut producer_process = StandardProcess::new(
            &mut producer_vspace,
            producer_cnode,
            producer_region,
            root_cnode,
            producer_run as extern "C" fn(_) -> (),
            producer_params,
            ut,
            ut,
            slots,
            tpa,
            None, // fault handler
        )?;
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
pub struct Xenon {
    a: u64,
}

pub struct ConsumerParams<Role: CNodeRole> {
    pub consumer: Consumer1<Role, Xenon>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ConsumerParams<role::Local> {
    type Output = ConsumerParams<role::Child>;
}

pub struct ProducerParams<Role: CNodeRole> {
    pub producer: Producer<Role, Xenon>,
}

impl RetypeForSetup for ProducerParams<role::Local> {
    type Output = ProducerParams<role::Child>;
}

pub extern "C" fn consumer_run(p: ConsumerParams<role::Local>) {
    let ConsumerParams {
        consumer,
        outcome_sender,
    } = p;
    let initial_state = 0;
    consumer.consume(
        initial_state,
        |state| {
            let fresh_state = state + 1;
            fresh_state
        },
        |x, state| {
            let fresh_state = x.a + state;
            if fresh_state > 10_000 {
                outcome_sender
                    .blocking_send(&true)
                    .expect("Failed to send test outcome");
            }
            fresh_state
        },
    )
}

pub extern "C" fn producer_run(p: ProducerParams<role::Local>) {
    for i in 0..256 {
        match p.producer.send(Xenon { a: i }) {
            Ok(_) => (),
            Err(QueueFullError(_x)) => {
                // Rejected sending this value, let's yield and let the consumer catch up
                // Note that we do not attempt to resend the rejected value
                unsafe {
                    selfe_sys::seL4_Yield();
                }
            }
        }
    }
}
