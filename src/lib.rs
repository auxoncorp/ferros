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
    role, root_cnode, BootInfo, CNode, LocalCap, SeL4Error, UnmappedPageTable, VSpace,
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
    let (ut16a, ut16b, _ut16c, _ut16d, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut16e, caller_ut, responder_ut, _, root_cnode) = ut18b.quarter(root_cnode)?;
    let (ut14, caller_thread_ut, responder_thread_ut, _, root_cnode) = ut16e.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) = ut14.quarter(root_cnode)?;
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

    let (caller_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16a.retype_local_cnode::<_, U12>(root_cnode)?;

    let (responder_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16b.retype_local_cnode::<_, U12>(root_cnode)?;

    // vspace setup
    let (caller_vspace, boot_info, root_cnode) = VSpace::new(boot_info, caller_ut, root_cnode)?;
    let (responder_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, responder_ut, root_cnode)?;

    let (
        caller_cnode_local,
        responder_cnode_local,
        caller,
        responder,
        caller_vspace,
        responder_vspace,
        root_cnode,
    ) = extended_call_channel(
        root_cnode,
        shared_page_ut,
        ut4a,
        ut4b,
        caller_vspace,
        responder_vspace,
        caller_cnode_local,
        responder_cnode_local,
    )
    .expect("ipc error");

    let caller_params = test_proc::ExtendedCallerParams::<role::Child> { caller };
    let responder_params = test_proc::ExtendedResponderParams::<role::Child> { responder };

    let (caller_thread, _caller_vspace, root_cnode) = caller_vspace
        .prepare_thread(
            test_proc::caller,
            caller_params,
            caller_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread a");

    caller_thread.start(caller_cnode_local, None, &boot_info.tcb, 255)?;

    let (responder_thread, _responder_vspace, _root_cnode) = responder_vspace
        .prepare_thread(
            test_proc::responder,
            responder_params,
            responder_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare child thread b");

    responder_thread.start(responder_cnode_local, None, &boot_info.tcb, 255)?;

    Ok(())
}
