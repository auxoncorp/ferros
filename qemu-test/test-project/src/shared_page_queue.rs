use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, Consumer1, FaultOrMessage, Producer, QueueFullError, RetypeForSetup,
    Sender,
};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn shared_page_queue(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U2>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_a_asid, asid_pool) = asid_pool.alloc();
        let (child_b_asid, _asid_pool) = asid_pool.alloc();

        let child_a_vspace = VSpace::new(ut, slots, child_a_asid, &user_image, &root_cnode)?;
        let child_b_vspace = VSpace::new(ut, slots, child_b_asid, &user_image, &root_cnode)?;

        let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_cnode, producer_slots) = retype_cnode::<U12>(ut, slots)?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (consumer, _consumer_token, producer_setup, _waker_setup, consumer_vspace) =
            Consumer1::new(
                ut,
                ut,
                child_a_vspace,
                local_vspace_scratch,
                &root_cnode,
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
        let (producer, producer_vspace) =
            Producer::new(&producer_setup, slots_p, child_b_vspace, &root_cnode, slots)?;

        let producer_params = ProducerParams::<role::Child> { producer };

        let (child_a_process, _) = consumer_vspace.prepare_thread(
            child_proc_a,
            consumer_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;
        child_a_process.start(consumer_cnode, Some(consumer_fault_source), tpa, 255)?;

        let (child_b_process, _) = producer_vspace.prepare_thread(
            child_proc_b,
            producer_params,
            ut,
            slots,
            local_vspace_scratch,
        )?;
        child_b_process.start(producer_cnode, None, tpa, 255)?;
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
    pub consumer: Consumer1<Role, Xenon, U100>,
    pub outcome_sender: Sender<bool, Role>,
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

pub extern "C" fn child_proc_a(p: ConsumerParams<role::Local>) {
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

pub extern "C" fn child_proc_b(p: ProducerParams<role::Local>) {
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
