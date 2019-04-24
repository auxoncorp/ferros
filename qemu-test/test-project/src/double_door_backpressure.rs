use super::TopLevelError;
use ferros::alloc::{self, smart_alloc, micro_alloc};
use ferros::userland::{
    role, root_cnode, BootInfo, CNodeRole, Consumer1, Consumer2, LocalCap, Producer,
    QueueFullError, RetypeForSetup, UnmappedPageTable, VSpace, Waker, retype, retype_cnode
};
use ferros::debug::DebugOutHandle;
use selfe_sys::{seL4_BootInfo, seL4_Yield};
use typenum::*;
type U4095 = Diff<U4096, U1>;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    // wrap all untyped memory
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;

    debug_println!("Allocator State: {:?}", allocator);

    // wrap root CNode for safe usage
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    // find an untyped of size 21 bits
    let ut21 = allocator
        .get_untyped::<U21>()
        .expect("initial alloc failure");
    let uts = alloc::ut_buddy(ut21);

    smart_alloc!(|slots from local_slots, ut from uts| {
        // wrap the rest of the critical boot info
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        // retypes
        let scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, boot_info) = boot_info.map_page_table(scratch_page_table)?;

        let (consumer_cnode, consumer_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_a_cnode, producer_a_slots) = retype_cnode::<U12>(ut, slots)?;
        let (producer_b_cnode, producer_b_slots) = retype_cnode::<U12>(ut, slots)?;
        let (waker_cnode, waker_slots) = retype_cnode::<U12>(ut, slots)?;

        // vspace setup
        let (consumer_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (producer_a_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (producer_b_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (waker_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;

        let (slots_c, consumer_slots) = consumer_slots.alloc();
        let (
            consumer,
            consumer_token,
            producer_setup_a,
            waker_setup,
            consumer_vspace,
        ) = Consumer1::new(
            ut,
            ut,
            consumer_vspace,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
            &root_cnode,
            slots,
            slots_c
        )?;

        let (consumer, producer_setup_b, consumer_vspace) = consumer.add_queue(
            &consumer_token,
            ut,
            consumer_vspace,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
            &root_cnode,
            slots
        )?;

        let consumer_params = ConsumerParams::<role::Child> { consumer };

        let (slots_a, producer_a_slots) = producer_a_slots.alloc();
        let (producer_a, producer_a_vspace) = Producer::new(
            &producer_setup_a,
            slots_a,
            producer_a_vspace,
            &root_cnode,
            slots
        )?;

        let producer_a_params = ProducerXParams::<role::Child> {
            producer: producer_a,
        };

        let (slots_b, producer_b_slots) = producer_b_slots.alloc();
        let (producer_b, producer_b_vspace) = Producer::new(
            &producer_setup_b,
            slots_b,
            producer_b_vspace,
            &root_cnode,
            slots
        )?;

        let producer_b_params = ProducerYParams::<role::Child> {
            producer: producer_b,
        };

        let (slots_w, waker_slots) = waker_slots.alloc();
        let waker = Waker::new(&waker_setup, slots_w, &root_cnode)?;
        let waker_params = WakerParams::<role::Child> { waker };

        let (consumer_thread, _) = consumer_vspace.prepare_thread(
            consumer_process,
            consumer_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        let (producer_a_thread, _) = producer_a_vspace.prepare_thread(
            producer_x_process,
            producer_a_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        let (producer_b_thread, _) = producer_b_vspace.prepare_thread(
            producer_y_process,
            producer_b_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        let (waker_thread, _) = waker_vspace.prepare_thread(
            waker_process,
            waker_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        consumer_thread.start(consumer_cnode, None, &boot_info.tcb, 255)?;
        producer_a_thread.start(producer_a_cnode, None, &boot_info.tcb, 255)?;
        producer_b_thread.start(producer_b_cnode, None, &boot_info.tcb, 255)?;
        waker_thread.start(waker_cnode, None, &boot_info.tcb, 255)?;
    });
    Ok(())
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
            debug_println!("Interrupt wakeup happened!");
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

pub extern "C" fn waker_process(p: WakerParams<role::Local>) {
    debug_println!("Inside waker");
    p.waker.send_wakeup_signal();
}

pub extern "C" fn producer_x_process(p: ProducerXParams<role::Local>) {
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

pub extern "C" fn producer_y_process(p: ProducerYParams<role::Local>) {
    debug_println!("Inside producer");
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
    debug_println!("\n\nProducer rejection count: {}\n\n", rejection_count);
}
