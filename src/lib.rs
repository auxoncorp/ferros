#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]

#[cfg(all(feature = "alloc"))]
#[macro_use]
extern crate alloc;

extern crate arrayvec;
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

use crate::micro_alloc::GetUntyped;
use crate::userland::{call_channel, role, root_cnode, spawn, BootInfo, CNode, LocalCap};
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
    // wrap all untyped memory
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)
        .expect("Couldn't set up bootstrap allocator");

    // wrap root CNode for safe usage
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode).expect("quarter");
    let (ut16a, ut16b, ut16c, ut16d, root_cnode) = ut18.quarter(root_cnode).expect("quarter");
    let (ut16e, _, _, _, root_cnode) = ut18b.quarter(root_cnode).expect("quarter");
    let (ut14, _, _, _, root_cnode) = ut16e.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, _, _, root_cnode) = ut14.quarter(root_cnode).expect("quarter");
    let (ut10, _, _, _, root_cnode) = ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");
    let (ut5, _, root_cnode) = ut6.split(root_cnode).expect("split");
    let (ut4, _, root_cnode) = ut5.split(root_cnode).expect("split"); // Why two splits? To exercise split.

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    let _root_cnode = {
        let (caller_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16a
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to caller_cnode_local");

        let (responder_cnode_local, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) = ut16b
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to responder_cnode_local");

        let (caller_cnode_local, responder_cnode_local, caller, responder, root_cnode) =
            call_channel(root_cnode, ut4, caller_cnode_local, responder_cnode_local)
                .expect("Could not make fastpath call channel");

        let (caller_cnode_child, caller_cnode_local) = caller_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("caller self awareness");
        let (responder_cnode_child, responder_cnode_local) = responder_cnode_local
            .generate_self_reference(&root_cnode)
            .expect("responder self awareness");

        let caller_params = test_proc::CallerParams::<role::Child> {
            my_cnode: caller_cnode_child,
            caller,
        };

        let responder_params = test_proc::ResponderParams::<role::Child> {
            my_cnode: responder_cnode_child,
            responder,
        };

        let root_cnode = spawn(
            test_proc::addition_requester,
            caller_params,
            caller_cnode_local,
            255, // priority
            ut16c,
            &mut boot_info,
            root_cnode,
        )
        .expect("spawn process 2");

        let root_cnode = spawn(
            test_proc::addition_responder,
            responder_params,
            responder_cnode_local,
            255, // priority
            ut16d,
            &mut boot_info,
            root_cnode,
        )
        .expect("spawn process 2");

        root_cnode
    };

    yield_forever();
}
