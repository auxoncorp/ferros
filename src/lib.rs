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

pub mod drivers;

use core::marker::PhantomData;
use crate::userland::{
    call_channel, role, root_cnode, spawn, Badge, BootInfo, CNode, CapRights, FaultSinkSetup,
    LocalCap, SeL4Error, UnmappedPage, UnmappedPageTable, VSpace,
};
use sel4_sys::*;
use typenum::{U12, U14, U20, U4096};

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

// uart base regs
// #define UART1_PADDR               0x02020000 /*   4 pages */
const UART1_PADDR: usize = 0x02020000;
// #define UART2_PADDR               0x021E8000 /*   4 pages */

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

    // get the uart device
    let uart_1_ut = allocator
        .get_device_untyped::<U14>(UART1_PADDR)
        .expect("find uart1 device memory");

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode)?;
    let (ut16a, ut16b, ut16c, ut16d, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut16e, echo_ut, driver_ut, _, root_cnode) = ut18b.quarter(root_cnode)?;
    let (ut14, echo_thread_ut, driver_thread_ut, _, root_cnode) = ut16e.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) = ut14.quarter(root_cnode)?;
    let (ut10, scratch_page_table_ut, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;
    let (ut5, _, root_cnode) = ut6.split(root_cnode)?;
    let (ut4, _, root_cnode) = ut5.split(root_cnode)?; // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    // scratch page for process spawning
    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

    // cnode allocation
    let (driver_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16b.retype_local_cnode::<_, U12>(root_cnode)?;

    let (echo_cnode, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
        ut16a.retype_local_cnode::<_, U12>(root_cnode)?;

    // ipc channel
    let (echo_cnode, driver_cnode, uart_client, uart_responder, root_cnode) =
        call_channel(root_cnode, ut4, echo_cnode, driver_cnode).expect("ipc error");

    ////////////////////
    // driver process //
    ////////////////////
    let (driver_vspace, mut boot_info, root_cnode) = VSpace::new(boot_info, echo_ut, root_cnode)?;

    let driver_config = crate::drivers::uart::basic::UARTConfig {
        register_base_addr: UART1_PADDR, // TODO: map 4 pages, and pass down the vaddr
        responder: uart_responder,
    };

    let (driver_thread, driver_vspace, root_cnode) = driver_vspace
        .prepare_thread(
            crate::drivers::uart::basic::run,
            driver_config,
            driver_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare uart driver thread");

    //////////////////
    // echo process //
    //////////////////
    let (driver_vspace, mut boot_info, root_cnode) = VSpace::new(boot_info, driver_ut, root_cnode)?;

    let echo_params = test_proc::EchoParams { uart: uart_client };

    let (echo_thread, echo_vspace, root_cnode) = driver_vspace
        .prepare_thread(
            test_proc::echo,
            echo_params,
            echo_thread_ut,
            root_cnode,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )
        .expect("prepare echo thread");

    // go!
    driver_thread.start(driver_cnode, None, &boot_info.tcb, 255);
    echo_thread.start(echo_cnode, None, &boot_info.tcb, 255);

    // caller setup
    // let (caller_shared_page, root_cnode) =
    //     shared_page.copy_inside_cnode(root_cnode, CapRights::RW)?;
    // let (caller_shared_page, caller_vspace) = caller_vspace.map_page(caller_shared_page)?;
    // let (caller_cnode_child, echo_cnode) =
    //     echo_cnode.generate_self_reference(&root_cnode)?;
    // let caller_params = test_proc::CallerParams::<role::Child> {
    //     my_cnode: caller_cnode_child,
    //     caller,
    //     shared_page: caller_shared_page.cap_data,
    // };

    // responder setup
    // let (responder_shared_page, root_cnode) =
    //     shared_page.copy_inside_cnode(root_cnode, CapRights::R)?;
    // let (responder_shared_page, responder_vspace) =
    //     responder_vspace.map_page(responder_shared_page)?;
    // let (responder_cnode_child, driver_cnode) =
    //     driver_cnode.generate_self_reference(&root_cnode)?;
    // let responder_params = test_proc::ResponderParams::<role::Child> {
    //     my_cnode: responder_cnode_child,
    //     responder,
    //     shared_page: responder_shared_page.cap_data,
    // };

    // cnode setup

    // let (caller_thread, caller_vspace, root_cnode) = caller_vspace
    //     .prepare_thread(
    //         test_proc::caller,
    //         caller_params,
    //         echo_thread_ut,
    //         root_cnode,
    //         &mut scratch_page_table,
    //         &mut boot_info.page_directory,
    //     )
    //     .expect("prepare child thread a");

    // caller_thread.start(echo_cnode, None, &boot_info.tcb, 255);

    // let (responder_thread, responder_vspace, root_cnode) = responder_vspace
    //     .prepare_thread(
    //         test_proc::responder,
    //         responder_params,
    //         driver_thread_ut,
    //         root_cnode,
    //         &mut scratch_page_table,
    //         &mut boot_info.page_directory,
    //     )
    //     .expect("prepare child thread a");

    // responder_thread.start(driver_cnode, None, &boot_info.tcb, 255);

    Ok(())
}
