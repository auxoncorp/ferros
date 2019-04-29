use super::TopLevelError;
use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::userland::{
    retype, retype_cnode, role, root_cnode, BootInfo, CNodeRole, Caller, Consumer1, Producer,
    QueueFullError, Responder, RetypeForSetup, VSpace,
};
use selfe_sys::*;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");
    let uts = alloc::ut_buddy(ut20);

    smart_alloc!(|slots from local_slots, ut from uts| {
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        let scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

        let (child_a_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (child_b_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
    });

    let (
        child_params_a,
        proc_cnode_local_a,
        child_a_vspace,
        child_fault_source_a,
        child_params_b,
        proc_cnode_local_b,
        child_b_vspace,
        child_fault_source_b,
        local_slots,
        uts,
    ) = {
        smart_alloc!(|slots from local_slots, ut from uts| {
            let (producer_cnode, producer_slots) = retype_cnode::<U12>(ut, slots)?;
            let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;

            let (slots_c, consumer_slots) = consumer_slots.alloc();
            let (
                consumer,
                _consumer_token,
                producer_setup,
                _waker_setup,
                consumer_vspace,
            ) = Consumer1::new(
                ut,
                ut,
                child_a_vspace,
                &mut scratch_page_table,
                &mut boot_info.page_directory,
                &root_cnode,
                slots,
                slots_c
            )?;

            let consumer_params = ConsumerParams::<role::Child> { consumer };

            let (slots_p, _producer_slots) = producer_slots.alloc();
            let (producer, producer_vspace) = Producer::new(
                &producer_setup,
                slots_p,
                child_b_vspace,
                &root_cnode,
                slots,
            )?;

            let producer_params = ProducerParams::<role::Child> { producer };

        });

        (
            consumer_params,
            consumer_cnode,
            consumer_vspace,
            None,
            producer_params,
            producer_cnode,
            producer_vspace,
            None,
            local_slots,
            uts,
        )
    };

    smart_alloc!(|slots from local_slots, ut from uts| {
        let (child_a_process, _) = child_a_vspace.prepare_thread(
            child_proc_a,
            child_params_a,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;
        child_a_process.start(
            proc_cnode_local_a,
            child_fault_source_a,
            &boot_info.tcb,
            255,
        )?;

        let (child_b_process, _) = child_b_vspace.prepare_thread(
            child_proc_b,
            child_params_b,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;
        child_b_process.start(
            proc_cnode_local_b,
            child_fault_source_b,
            &boot_info.tcb,
            255,
        )?;
    });

    Ok(())
}

#[derive(Debug)]
pub struct AdditionRequest {
    a: u32,
    b: u32,
}

#[derive(Debug)]
pub struct AdditionResponse {
    sum: u32,
}

#[derive(Debug)]
pub struct CallerParams<Role: CNodeRole> {
    pub caller: Caller<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for CallerParams<role::Local> {
    type Output = CallerParams<role::Child>;
}

#[derive(Debug)]
pub struct ResponderParams<Role: CNodeRole> {
    pub responder: Responder<AdditionRequest, AdditionResponse, Role>,
}

impl RetypeForSetup for ResponderParams<role::Local> {
    type Output = ResponderParams<role::Child>;
}

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

pub extern "C" fn child_proc_a(p: ConsumerParams<role::Local>) {
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

pub extern "C" fn child_proc_b(p: ProducerParams<role::Local>) {
    debug_println!("Inside producer");
    for i in 0..256 {
        match p.producer.send(Xenon { a: i }) {
            Ok(_) => {
                debug_println!("The producer *thinks* it successfully sent {}", i);
            }
            Err(QueueFullError(x)) => {
                debug_println!("Rejected sending {:?}", x);
                unsafe {
                    seL4_Yield();
                }
            }
        }
        debug_println!("done producing!");
    }
}
