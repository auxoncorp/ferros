#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]
// Necessary to mark as not-Send or not-Sync
#![feature(optin_builtin_traits)]
#![feature(associated_type_defaults)]

#[cfg(all(feature = "alloc"))]
#[macro_use]
extern crate alloc;

extern crate arrayvec;
extern crate generic_array;
extern crate sel4_sys;
extern crate typenum;

extern crate cross_queue;

#[cfg(all(feature = "test"))]
extern crate proptest;

#[cfg(feature = "test")]
pub mod fel4_test;

#[macro_use]
mod debug;

pub mod micro_alloc;
pub mod pow;
mod twinkle_types;
pub mod userland;

mod test_proc;

use crate::micro_alloc::GetUntyped;
use crate::userland::sync::extended_call_channel;
use crate::userland::{
    create_double_door, role, root_cnode, BootInfo, CNode, Consumer1, LocalCap, Producer,
    ProducerSetup, SeL4Error, UnmappedPageTable, VSpace, Waker, WakerSetup,
};
use sel4_sys::*;
use typenum::{U100, U12, U20, U4096};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub fn run(raw_boot_info: &'static seL4_BootInfo) {
    do_run(raw_boot_info).expect("run error");
    yield_forever();
}

fn do_run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), SeL4Error> {
    // wrap all untyped memory
    let mut allocator =
        micro_alloc::Allocator::bootstrap(&raw_boot_info).expect("bootstrap failure");

    // wrap root CNode for safe usage
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("initial alloc failure");

    let (ut18, ut18b, ut18c, _, root_cnode) = ut20.quarter(root_cnode)?;
    let (ut16a, ut16b, ut16c, ut16d, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut16e, caller_ut, producer_a_ut, waker_ut, root_cnode) = ut18b.quarter(root_cnode)?;
    let (ut16i, producer_b_ut, _, _, root_cnode) = ut18c.quarter(root_cnode)?;
    let (ut14a, consumer_thread_ut, producer_a_thread_ut, waker_thread_ut, root_cnode) =
        ut16e.quarter(root_cnode)?;
    let (_ut14e, producer_b_thread_ut, _, _, root_cnode) = ut16i.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) = ut14a.quarter(root_cnode)?;
    let (ut10, scratch_page_table_ut, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;
    let (ut5, _, root_cnode) = ut6.split(root_cnode)?;
    let (ut4a, ut4b, root_cnode) = ut5.split(root_cnode)?; // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    // retypes
    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    let (mut scratch_page_table, boot_info) = boot_info.map_page_table(scratch_page_table)?;

    let (consumer_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16a.retype_cnode::<_, U12>(root_cnode)?;

    let (producer_a_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16b.retype_cnode::<_, U12>(root_cnode)?;
    let (producer_b_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16c.retype_cnode::<_, U12>(root_cnode)?;

    let (waker_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16d.retype_cnode::<_, U12>(root_cnode)?;

    // vspace setup
    let (consumer_vspace, boot_info, root_cnode) = VSpace::new(boot_info, caller_ut, root_cnode)?;
    let (producer_a_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, producer_a_ut, root_cnode)?;
    let (producer_b_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, producer_b_ut, root_cnode)?;

    let (waker_vspace, mut boot_info, root_cnode) = VSpace::new(boot_info, waker_ut, root_cnode)?;

    let (consumer, producer_setup, waker_setup, consumer_cnode, consumer_vspace, root_cnode) =
        create_double_door(
            shared_page_ut,
            ut4a,
            consumer_cnode,
            consumer_vspace,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
            root_cnode,
        )
        .expect("ipc error");

    let consumer_params = test_proc::ConsumerParams::<role::Child> { consumer };

    let (producer_a, producer_a_cnode, producer_a_vspace, root_cnode) = Producer::new(
        &producer_setup,
        producer_a_cnode,
        producer_a_vspace,
        root_cnode,
    )?;
    let producer_a_params = test_proc::ProducerParams::<role::Child> {
        producer: producer_a,
    };

    let (producer_b, producer_b_cnode, producer_b_vspace, root_cnode) = Producer::new(
        &producer_setup,
        producer_b_cnode,
        producer_b_vspace,
        root_cnode,
    )?;
    let producer_b_params = test_proc::ProducerParams::<role::Child> {
        producer: producer_b,
    };

    let (waker, waker_cnode) = Waker::new(&waker_setup, waker_cnode, &root_cnode)?;
    let waker_params = test_proc::WakerParams::<role::Child> { waker };

    let (consumer_thread, _consumer_vspace, root_cnode) = consumer_vspace
        .prepare_thread(
            test_proc::consumer,
            consumer_params,
            consumer_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread a");

    consumer_thread.start(consumer_cnode, None, &boot_info.tcb, 255)?;

    let (producer_a_thread, _producer_a_vspace, root_cnode) = producer_a_vspace
        .prepare_thread(
            test_proc::producer,
            producer_a_params,
            producer_a_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread b");

    producer_a_thread.start(producer_a_cnode, None, &boot_info.tcb, 255)?;

    let (producer_b_thread, _producer_b_vspace, root_cnode) = producer_b_vspace
        .prepare_thread(
            test_proc::producer,
            producer_b_params,
            producer_b_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread b");

    producer_b_thread.start(producer_b_cnode, None, &boot_info.tcb, 255)?;

    let (waker_thread, _waker_vspace, _root_cnode) = waker_vspace
        .prepare_thread(
            test_proc::waker,
            waker_params,
            waker_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread b");

    waker_thread.start(waker_cnode, None, &boot_info.tcb, 255)?;

    Ok(())
}
