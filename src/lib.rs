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
mod pow;
mod twinkle_types;
pub mod userland;

mod test_proc;

use crate::micro_alloc::GetUntyped;
use crate::userland::{
    role, root_cnode, spawn, ASIDControl, ASIDPool, AssignedPageDirectory, Cap, Endpoint,
    MappedPage, ThreadControlBlock,
};
use sel4_sys::*;
use typenum::{U12, U20};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub fn run(bootinfo: &'static seL4_BootInfo) {
    let mut allocator =
        micro_alloc::Allocator::bootstrap(&bootinfo).expect("Couldn't set up bootstrap allocator");

    // wrap bootinfo caps
    let root_cnode = root_cnode(&bootinfo);
    let mut root_page_directory =
        Cap::<AssignedPageDirectory, _>::wrap_cptr(seL4_CapInitThreadVSpace as usize);
    let root_tcb = Cap::<ThreadControlBlock, _>::wrap_cptr(seL4_CapInitThreadTCB as usize);
    let user_image_pages_iter = (bootinfo.userImageFrames.start..bootinfo.userImageFrames.end)
        .map(|cptr| Cap::<MappedPage, role::Local>::wrap_cptr(cptr as usize));

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, _, _, _, root_cnode) = ut20.quarter(root_cnode).expect("quarter");
    let (ut16, child_cnode_ut, child_proc_ut, _, root_cnode) =
        ut18.quarter(root_cnode).expect("quarter");
    let (ut14, _, _, _, root_cnode) = ut16.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, stack_ut, _, root_cnode) = ut14.quarter(root_cnode).expect("quarter");
    let (ut10, _, _, _, root_cnode) = ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");
    let (fault_ep_ut, _, _, _, root_cnode) = ut6.quarter(root_cnode).expect("quarter");

    // asid control
    let asid_control = Cap::<ASIDControl, _>::wrap_cptr(seL4_CapASIDControl as usize);

    // asid pool
    let (mut asid_pool, root_cnode): (Cap<ASIDPool, _>, _) = asid_pool_ut
        .retype_asid_pool(asid_control, root_cnode)
        .expect("retype asid pool");

    // fault endpoint
    let (fault_ep, root_cnode): (Cap<Endpoint, _>, _) = fault_ep_ut
        .retype_local(root_cnode)
        .expect("retype fault endpoint");

    // child cnode
    let (child_cnode, root_cnode) = child_cnode_ut
        .retype_local_cnode::<_, U12>(root_cnode)
        .expect("Couldn't retype to child proc cnode");

    let mut nums = [0xaaaaaaaa; 140];
    nums[0] = 0xbbbbbbbb;
    nums[139] = 0xcccccccc;
    let params = test_proc::Params { nums };

    let _root_cnode = spawn(
        test_proc::main,
        params,
        child_cnode,
        255, // priority
        stack_ut,
        child_proc_ut,
        &mut asid_pool,
        &mut root_page_directory,
        user_image_pages_iter,
        root_tcb,
        &fault_ep,
        root_cnode,
    )
    .expect("spawn process");

    loop {
        let mut sender = 42usize;
        let mut message_info: seL4_MessageInfo_t = unsafe { seL4_Recv(fault_ep.cptr, &mut sender) };
        let label = unsafe { seL4_MessageInfo_ptr_get_label(&mut message_info) };
        let caps_unwrapped = unsafe { seL4_MessageInfo_ptr_get_capsUnwrapped(&mut message_info) };
        let extra_caps = unsafe { seL4_MessageInfo_ptr_get_extraCaps(&mut message_info) };
        let length = unsafe { seL4_MessageInfo_ptr_get_length(&mut message_info) };

        debug_println!("Received fault");
        debug_println!("  sender: {:08x}", sender);
        debug_println!("  label: {}", label);
        debug_println!("  caps_unwrapped: {:08x}", caps_unwrapped);
        debug_println!("  extra_caps: {:08x}", extra_caps);
        debug_println!("  length: {}", length);
    }

    yield_forever();
}
