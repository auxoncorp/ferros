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
    role, root_cnode, spawn, ASIDControl, ASIDPool, AssignedPageDirectory, CNode, Cap, LocalCap,
    MappedPage, ThreadControlBlock,
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

pub fn run(bootinfo: &'static seL4_BootInfo) {
    let mut allocator =
        micro_alloc::Allocator::bootstrap(&bootinfo).expect("Couldn't set up bootstrap allocator");

    // wrap bootinfo caps
    let root_cnode = root_cnode(&bootinfo);
    let mut root_page_directory =
        Cap::<AssignedPageDirectory, _>::wrap_cptr(seL4_CapInitThreadVSpace as usize);
    let root_tcb = Cap::<ThreadControlBlock, _>::wrap_cptr(seL4_CapInitThreadTCB as usize);
    // TODO - wrap the creation of this iterator, probably giving it a proper name
    let user_image_pages_iter_a = (bootinfo.userImageFrames.start..bootinfo.userImageFrames.end)
        .map(|cptr| Cap::<MappedPage, role::Local>::wrap_cptr(cptr as usize));
    let user_image_pages_iter_b = (bootinfo.userImageFrames.start..bootinfo.userImageFrames.end)
        .map(|cptr| Cap::<MappedPage, role::Local>::wrap_cptr(cptr as usize));

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode).expect("quarter");
    let (ut16, child_cnode_ut, child_proc_ut, _, root_cnode) =
        ut18.quarter(root_cnode).expect("quarter");
    let (child_cnode_ut_b, child_proc_ut_b, _, _, root_cnode) =
        ut18b.quarter(root_cnode).expect("quarter");
    let (ut14, _, _, _, root_cnode) = ut16.quarter(root_cnode).expect("quarter");
    let (ut12, asid_pool_ut, _, _, root_cnode) = ut14.quarter(root_cnode).expect("quarter");
    let (ut10, _, _, _, root_cnode) = ut12.quarter(root_cnode).expect("quarter");
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode).expect("quarter");
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");

    // asid control
    let asid_control = Cap::<ASIDControl, _>::wrap_cptr(seL4_CapASIDControl as usize);

    // asid pool
    let (mut asid_pool, root_cnode): (Cap<ASIDPool, _>, _) = asid_pool_ut
        .retype_asid_pool(asid_control, root_cnode)
        .expect("retype asid pool");

    let root_cnode = {
        // child process demonstrating passing stack-starting data
        // that exceeds the amount one could fit in the registers
        let (child_cnode, root_cnode) = child_cnode_ut
            .retype_local_cnode::<_, U12>(root_cnode)
            .expect("Couldn't retype to child proc cnode");

        let mut nums = [0xaaaaaaaa; 140];
        nums[0] = 0xbbbbbbbb;
        nums[139] = 0xcccccccc;
        let params = test_proc::OverRegisterSizeParams { nums };

        spawn(
            test_proc::param_size_run,
            params,
            child_cnode,
            255, // priority
            child_proc_ut,
            &mut asid_pool,
            &mut root_page_directory,
            user_image_pages_iter_a,
            &root_tcb,
            root_cnode,
        )
        .expect("spawn process")
    };

    let _root_cnode = {
        // child process demonstrating that we can wire up
        // passing capability objects to child processes
        let (child_cnode_b, root_cnode): (LocalCap<CNode<U4096, role::Child>>, _) =
            child_cnode_ut_b
                .retype_local_cnode::<_, U12>(root_cnode)
                .expect("Couldn't retype to child2 proc cnode");

        let (child_ut6, child_cnode_b) = ut6
            .move_to_cnode(&root_cnode, child_cnode_b)
            .expect("move untyped into child cnode b");

        let (child_cnode_b_child, child_cnode_b_local) = child_cnode_b
            .generate_self_reference(&root_cnode)
            .expect("self awareness");

        let parent_params = test_proc::CapManagementParams::<role::Child> {
            num: 17,
            //process_start_context: child_process_start_context,
            my_cnode: child_cnode_b_child,
            data_source: child_ut6,
        };

        spawn(
            test_proc::cap_management_run,
            parent_params,
            child_cnode_b_local,
            255, // priority
            child_proc_ut_b,
            &mut asid_pool,
            &mut root_page_directory,
            user_image_pages_iter_b,
            &root_tcb,
            root_cnode,
        )
        .expect("spawn process 2")
    };

    yield_forever();
}
