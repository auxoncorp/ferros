#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]

#[cfg(all(feature = "alloc"))]
#[macro_use]
extern crate alloc;

extern crate arrayvec;
extern crate generic_array;
extern crate sel4_sys;
extern crate typenum;

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

use core::marker::PhantomData;
use crate::micro_alloc::GetUntyped;
use crate::userland::{
    call_channel, role, root_cnode, spawn, Badge, BootInfo, CNode, CapRights, FaultSinkSetup,
    LocalCap, SeL4Error, UnmappedPage, UnmappedPageTable, VSpace,
};
use sel4_sys::*;
use typenum::{U12, U20, U4096};

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

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode)?;
    let (ut16a, ut16b, ut16c, ut16d, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut16e, caller_ut, responder_ut, _, root_cnode) = ut18b.quarter(root_cnode)?;
    let (ut14, caller_thread_ut, responder_thread_ut, _, root_cnode) = ut16e.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) = ut14.quarter(root_cnode)?;
    let (ut10, scratch_page_table_ut, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;
    let (ut5, _, root_cnode) = ut6.split(root_cnode)?;
    let (ut4, _, root_cnode) = ut5.split(root_cnode)?; // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    // retypes
    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

    let (caller_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16a.retype_local_cnode::<_, U12>(root_cnode)?;

    let (responder_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16b.retype_local_cnode::<_, U12>(root_cnode)?;

    let (caller_cnode_local, responder_cnode_local, caller, responder, root_cnode) =
        call_channel(root_cnode, ut4, caller_cnode_local, responder_cnode_local)
            .expect("ipc error");

    // vspace setup
    let (caller_vspace, mut boot_info, root_cnode) = VSpace::new(boot_info, caller_ut, root_cnode)?;

    let (responder_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, responder_ut, root_cnode)?;

    // set up shm page caps
    let (shared_page, root_cnode): (LocalCap<UnmappedPage>, _) =
        shared_page_ut.retype_local(root_cnode)?;

    // caller setup
    let (caller_shared_page, root_cnode) =
        shared_page.copy_inside_cnode(root_cnode, CapRights::RW)?;
    let (caller_shared_page, caller_vspace) = caller_vspace.map_page(caller_shared_page)?;
    let (caller_cnode_child, caller_cnode_local) =
        caller_cnode_local.generate_self_reference(&root_cnode)?;
    let caller_params = test_proc::CallerParams::<role::Child> {
        my_cnode: caller_cnode_child,
        caller,
        shared_page: caller_shared_page.cap_data,
    };

    // responder setup
    let (responder_shared_page, root_cnode) =
        shared_page.copy_inside_cnode(root_cnode, CapRights::R)?;
    let (responder_shared_page, responder_vspace) =
        responder_vspace.map_page(responder_shared_page)?;
    let (responder_cnode_child, responder_cnode_local) =
        responder_cnode_local.generate_self_reference(&root_cnode)?;
    let responder_params = test_proc::ResponderParams::<role::Child> {
        my_cnode: responder_cnode_child,
        responder,
        shared_page: responder_shared_page.cap_data,
    };

    // cnode setup

    let (child_a_thread, caller_vspace, root_cnode) = caller_vspace
        .prepare_thread(
            test_proc::child_proc_a,
            caller_params,
            caller_thread_ut,
            root_cnode,
        )
        .expect("prepare child thread a");

    child_a_thread.start(caller_cnode_local, None, &boot_info.tcb, 255);

    let (child_b_thread, responder_vspace, root_cnode) = responder_vspace
        .prepare_thread(
            test_proc::child_proc_b,
            responder_params,
            responder_thread_ut,
            root_cnode,
        )
        .expect("prepare child thread a");

    child_b_thread.start(responder_cnode_local, None, &boot_info.tcb, 255);

    // let root_cnode = spawn(
    //     test_proc::child_proc_a,
    //     caller_params,
    //     caller_cnode_local,
    //     255,  // priority
    //     None, // fault_source
    //     ut16c,
    //     &mut boot_info,
    //     &mut scratch_page_table,
    //     root_cnode,
    // )?;

    // let root_cnode = spawn(
    //     test_proc::child_proc_b,
    //     responder_params,
    //     responder_cnode_local,
    //     255,  // priority
    //     None, // fault_source
    //     ut16d,
    //     &mut boot_info,
    //     &mut scratch_page_table,
    //     root_cnode,
    // )?;

    Ok(())
}
