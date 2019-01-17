#![no_std]

extern crate iron_pegasus;
extern crate sel4_sys;
extern crate typenum;

use sel4_sys::*;

macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}

use iron_pegasus::micro_alloc::{self, GetUntyped};
use iron_pegasus::userland::{
    role, root_cnode, spawn, ASIDControl, ASIDPool, AssignedPageDirectory, Cap, MappedPage,
    ThreadControlBlock,
};
use typenum::{U12, U20};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub fn run(bootinfo: &'static seL4_BootInfo) {
    #[cfg(test_case = "root_task_runs")]
    {
        debug_println!("\nhello from the root task!\n");
    }

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
    let (_ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode).expect("quarter");

    // asid control
    let asid_control = Cap::<ASIDControl, _>::wrap_cptr(seL4_CapASIDControl as usize);

    // asid pool
    let (mut asid_pool, root_cnode): (Cap<ASIDPool, _>, _) = asid_pool_ut
        .retype_asid_pool(asid_control, root_cnode)
        .expect("retype asid pool");

    // child cnode
    let (child_cnode, root_cnode) = child_cnode_ut
        .retype_local_cnode::<_, U12>(root_cnode)
        .expect("Couldn't retype to child proc cnode");

    let params = ProcParams { value: 42 };

    let _root_cnode = spawn(
        proc_main,
        params,
        child_cnode,
        255, // priority
        stack_ut,
        child_proc_ut,
        &mut asid_pool,
        &mut root_page_directory,
        user_image_pages_iter,
        root_tcb,
        root_cnode,
    )
    .expect("spawn process");

    yield_forever();
}

pub struct ProcParams {
    pub value: usize,
}

impl iron_pegasus::userland::RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

// 'extern' to force C calling conventions
pub extern "C" fn proc_main(params: &ProcParams) {
    #[cfg(test_case = "process_runs")]
    {
        debug_println!("\nThe value inside the process is {}\n", params.value);
    }

    #[cfg(test_case = "memory_read_protection")]
    {
        debug_println!("\nAttempting to cause a segmentation fault...\n");

        unsafe {
            let x: *const usize = 0x88888888usize as _;
            debug_println!("Value from arbitrary memory is: {}", *x);
        }

        debug_println!("This is after the segfaulting code, and should not be printed.");
    }

    #[cfg(test_case = "memory_write_protection")]
    {
        debug_println!("\nAttempting to write to the code segment...\n");

        unsafe {
            let x: *mut usize = proc_main as _;
            *x = 42;
        }

        debug_println!("This is after the segfaulting code, and should not be printed.");
    }
}
