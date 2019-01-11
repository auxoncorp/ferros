// NOTE: this file is generated by fel4
// NOTE: Don't edit it here; your changes will be lost at the next build!
#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]
#![feature(lang_items, core_intrinsics)]
#![feature(global_asm)]
#![feature(panic_info_message)]

#[cfg(feature = "alloc")]
extern crate alloc;
extern crate iron_pegasus;
#[cfg(all(feature = "test", feature = "alloc"))]
extern crate proptest;
extern crate sel4_sys;
#[cfg(feature = "alloc")]
extern crate wee_alloc;

use core::alloc::Layout;
use core::intrinsics;
use core::mem;
use core::panic::PanicInfo;
use sel4_sys::*;

#[cfg(feature = "alloc")]
#[global_allocator]
static ALLOCATOR: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

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

// include the seL4 kernel configurations
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
pub mod sel4_config {
    pub const KernelDebugBuild: bool = true;
    pub const KernelTimerTickMS: &'static str = "2";
    pub const KernelArmExportPMUUser: bool = false;
    pub const BuildWithCommonSimulationSettings: bool = true;
    pub const HardwareDebugAPI: bool = false;
    pub const KernelDebugDisableL2Cache: bool = false;
    pub const LibSel4FunctionAttributes: &'static str = "public";
    pub const KernelMaxNumNodes: &'static str = "1";
    pub const KernelFPUMaxRestoresSinceSwitch: &'static str = "64";
    pub const KernelStackBits: &'static str = "12";
    pub const KernelBenchmarks: &'static str = "none";
    pub const KernelColourPrinting: bool = true;
    pub const ElfloaderMode: &'static str = "secure supervisor";
    pub const KernelNumPriorities: &'static str = "256";
    pub const LinkPageSize: &'static str = "4096";
    pub const KernelVerificationBuild: bool = false;
    pub const LibSel4DebugFunctionInstrumentation: &'static str = "none";
    pub const UserLinkerGCSections: bool = false;
    pub const KernelIPCBufferLocation: &'static str = "threadID_register";
    pub const KernelRootCNodeSizeBits: &'static str = "19";
    pub const KernelFWholeProgram: bool = false;
    pub const ElfloaderErrata764369: bool = true;
    pub const KernelMaxNumBootinfoUntypedCaps: &'static str = "230";
    pub const KernelARMPlatform: &'static str = "sabre";
    pub const KernelPrinting: bool = true;
    pub const KernelDebugDisableBranchPrediction: bool = false;
    pub const KernelUserStackTraceLength: &'static str = "16";
    pub const KernelAArch32FPUEnableContextSwitch: bool = true;
    pub const KernelArmEnableA9Prefetcher: bool = false;
    pub const KernelArch: &'static str = "arm";
    pub const ElfloaderImage: &'static str = "elf";
    pub const KernelRetypeFanOutLimit: &'static str = "256";
    pub const LibSel4DebugAllocBufferEntries: &'static str = "0";
    pub const KernelTimeSlice: &'static str = "5";
    pub const KernelFastpath: bool = true;
    pub const KernelArmSel4Arch: &'static str = "aarch32";
    pub const KernelResetChunkBits: &'static str = "8";
    pub const KernelNumDomains: &'static str = "1";
    pub const KernelMaxNumWorkUnitsPerPreemption: &'static str = "100";
    pub const KernelOptimisation: &'static str = "-O2";
}

pub static mut BOOTINFO: *mut seL4_BootInfo = (0 as *mut seL4_BootInfo);
static mut RUN_ONCE: bool = false;

#[no_mangle]
pub unsafe extern "C" fn __sel4_start_init_boot_info(bootinfo: *mut seL4_BootInfo) {
    if !RUN_ONCE {
        BOOTINFO = bootinfo;
        RUN_ONCE = true;
        seL4_SetUserData((*bootinfo).ipcBuffer as usize as seL4_Word);
    }
}

#[lang = "termination"]
trait Termination {
    fn report(self) -> i32;
}

impl Termination for () {
    fn report(self) -> i32 {
        0
    }
}

#[lang = "start"]
fn lang_start<T: Termination + 'static>(
    main: fn() -> T,
    _argc: isize,
    _argv: *const *const u8,
) -> isize {
    main();
    panic!("Root task should never return from main!");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    #[cfg(feature = "KernelPrinting")]
    {
        use core::fmt::Write;

        if let Some(loc) = info.location() {
            let _ = write!(
                sel4_sys::DebugOutHandle,
                "panic at {}:{}: ",
                loc.file(),
                loc.line()
            );
        } else {
            let _ = write!(sel4_sys::DebugOutHandle, "panic: ");
        }

        if let Some(fmt) = info.message() {
            let _ = sel4_sys::DebugOutHandle.write_fmt(*fmt);
        }
        let _ = sel4_sys::DebugOutHandle.write_char('\n');

        let _ = write!(
            sel4_sys::DebugOutHandle,
            "----- aborting from panic -----\n"
        );
    }
    unsafe { intrinsics::abort() }
}

#[lang = "eh_personality"]
#[no_mangle]
pub fn eh_personality() {
    #[cfg(feature = "KernelPrinting")]
    {
        use core::fmt::Write;
        let _ = write!(
            sel4_sys::DebugOutHandle,
            "----- aborting from eh_personality -----\n"
        );
    }
    unsafe {
        core::intrinsics::abort();
    }
}

#[lang = "oom"]
#[no_mangle]
pub extern "C" fn oom(_layout: Layout) -> ! {
    #[cfg(feature = "KernelPrinting")]
    {
        use core::fmt::Write;
        let _ = write!(
            sel4_sys::DebugOutHandle,
            "----- aborting from out-of-memory -----\n"
        );
    }
    unsafe { core::intrinsics::abort() }
}

const CHILD_STACK_SIZE: usize = 4096;
static mut CHILD_STACK: *const [u64; CHILD_STACK_SIZE] = &[0; CHILD_STACK_SIZE];

// fn split_untyped(untyped: Untyped<U4>) -> (Untyped<U3>, Untyped<U3>) {

// }

// fn make_tcb(untyped: Untyped<U3>, ....) -> Option<TCB> {

// }

// fn make_endpoint(untyped: Untyped<U3>, ....) -> Endpoint {

// }

// struct Process {
//     // the shit it needs
//     stack: u8[1024],
//     caps: (Endpoint, something...),
//     children: (...)
// }

// trait Process<Req, Res> {
//     fn size_bits() -> usize;
//     fn call(&self, Req) -> Res;
// }

// fn spawn(parent_cnode, mem: Untyped<U5>, endpoint: Endpoint) {
//     let (local_mem, child_mem) = mem.split();
//     let cnode = make_cnode(local_mem, parent);
//     let child_endpoint = endpoint.derive_into(cnode);
//     let child_endpoint = endpoint.derive_into(cnode);
// }

// {
//     let ep = make_endpoint(ut);
//     let child1_ep = ep.derive_into(child1_cnode);
//     let child2_ep = ep.derive_into(child2_cnode);
// }

use iron_pegasus::fancy::{
    self, wrap_untyped, CNode, Capability, ChildCapability, Endpoint, ThreadControlBlock, Untyped,
};
use iron_pegasus::micro_alloc::{self, GetUntyped};
use typenum::{U19, U20, U8};

fn main() {
    let bootinfo = unsafe { &*BOOTINFO };
    let root_cnode = fancy::root_cnode(&bootinfo);
    let mut allocator =
        micro_alloc::Allocator::bootstrap(&bootinfo).expect("Couldn't set up bootstrap allocator");

    debug_println!("Made root cnode: {:?}\n\n", root_cnode);

    // find an untyped of size 20 bits (1 meg)
    let one_meg = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (half_meg_1, half_meg_2, root_cnode) = one_meg
        .split(root_cnode)
        .expect("Couldn't split untyped half megs");
    debug_println!(
        "split thing into half megs: {:?} {:?} {:?}\n\n",
        half_meg_1,
        half_meg_2,
        root_cnode
    );

    let (quarter_meg_1, quarter_meg_2, root_cnode) = half_meg_1
        .split(root_cnode)
        .expect("Couldn't split untyped quarter megs first time");
    debug_println!(
        "split thing into quarter megs first time: {:?} {:?} {:?}\n\n",
        quarter_meg_1,
        quarter_meg_2,
        root_cnode
    );

    let (quarter_meg_3, quarter_meg_4, root_cnode) = half_meg_2
        .split(root_cnode)
        .expect("Couldn't split untyped quarter megs second time");
    debug_println!(
        "split thing into quarter megs second time: {:?} {:?} {:?}\n\n",
        quarter_meg_3,
        quarter_meg_4,
        root_cnode
    );

    let (child_cnode, root_cnode): (Capability<CNode<U8, _, _>>, _) = quarter_meg_2
        .retype_local_cnode(root_cnode)
        .expect("Couldn't retype to child proc cnode");
    debug_println!(
        "retyped child proc cnode {:?} {:?}\n\n",
        child_cnode,
        root_cnode
    );

    let (child_endpoint, child_cnode): (ChildCapability<Endpoint>, _) = quarter_meg_3
        .retype_child(child_cnode)
        .expect("Couldn't retype to child proc endpoint");
    debug_println!(
        "retyped child proc endpoint {:?} {:?}\n\n",
        child_endpoint,
        root_cnode
    );

    let (mut child_tcb, root_cnode): (Capability<ThreadControlBlock>, _) = quarter_meg_1
        .retype_local(root_cnode)
        .expect("couldn't retyped to tcb");
    debug_println!(
        "retyped as thread control block {:?} {:?}\n\n",
        child_tcb,
        root_cnode
    );

    child_tcb
        .configure(child_cnode, seL4_CapInitThreadVSpace)
        .expect("Couldn't configure child tcb");

    debug_println!("conigured child tcb",);

    // let suspend_err = unsafe { seL4_TCB_Suspend(seL4_CapInitThreadTCB) };
    // assert!(suspend_err == 0);


    let stack_base = unsafe { CHILD_STACK as usize };
    let stack_top = stack_base + CHILD_STACK_SIZE;
    let mut regs: seL4_UserContext = unsafe { mem::zeroed() };
    #[cfg(feature = "test")]
    {
        regs.pc = iron_pegasus::fel4_test::run as seL4_Word;
    }
    #[cfg(not(feature = "test"))]
    {
        regs.pc = iron_pegasus::run as seL4_Word;
    }
    regs.sp = stack_top as seL4_Word;

    let _: u32 = unsafe { seL4_TCB_WriteRegisters(child_tcb.cptr as u32, 0, 0, 2, &mut regs) };
    let _: u32 = unsafe { seL4_TCB_SetPriority(child_tcb.cptr as u32, seL4_CapInitThreadTCB.into(), 255) };
    let _: u32 = unsafe { seL4_TCB_Resume(child_tcb.cptr as u32) };
    loop {
        unsafe {
            seL4_Yield();
        }
    }

    ///////////////// old code

    // let bootinfo = unsafe { &*BOOTINFO };
    // let cspace_cap = seL4_CapInitThreadCNode;
    // let pd_cap = seL4_CapInitThreadVSpace;
    // let tcb_cap = bootinfo.empty.start;

    // let mut allocator = iron_pegasus::allocator::Allocator::bootstrap(&bootinfo)
    //     .expect("Failed to create bootstrap allocator");

    // let untyped = allocator
    //     .alloc_untyped(seL4_TCBBits as usize, None, false)
    //     .unwrap();

    // let tcb_cap = allocator
    //     .retype_untyped_memory(untyped, api_object_seL4_TCBObject, seL4_TCBBits as usize, 1)
    //     .expect("Failed to retype untyped memory")
    //     .first as u32;

    // // let tcb_cap = allocator.gimme<TCB>();
    // // let tcb = TCB::new(&allocator)?;

    // let cspace_cap = seL4_CapInitThreadCNode;
    // let pd_cap = seL4_CapInitThreadVSpace;

    // let tcb_err: seL4_Error = unsafe {
    //     seL4_TCB_Configure(
    //         tcb_cap,
    //         seL4_CapNull.into(),
    //         cspace_cap.into(),
    //         seL4_NilData.into(),
    //         pd_cap.into(),
    //         seL4_NilData.into(),
    //         0,
    //         0,
    //     )
    // };

    // assert!(tcb_err == 0, "Failed to configure TCB");

    // let stack_base = unsafe { CHILD_STACK as usize };
    // let stack_top = stack_base + CHILD_STACK_SIZE;
    // let mut regs: seL4_UserContext = unsafe { mem::zeroed() };
    // #[cfg(feature = "test")]
    // {
    //     regs.pc = iron_pegasus::fel4_test::run as seL4_Word;
    // }
    // #[cfg(not(feature = "test"))]
    // {
    //     regs.pc = iron_pegasus::run as seL4_Word;
    // }
    // regs.sp = stack_top as seL4_Word;

    // let _: u32 = unsafe { seL4_TCB_WriteRegisters(tcb_cap, 0, 0, 2, &mut regs) };
    // let _: u32 = unsafe { seL4_TCB_SetPriority(tcb_cap, seL4_CapInitThreadTCB.into(), 255) };
    // let _: u32 = unsafe { seL4_TCB_Resume(tcb_cap) };
    // loop {
    //     unsafe {
    //         seL4_Yield();
    //     }
    // }
}

global_asm!(
    r###"/* Copyright (c) 2015 The Robigalia Project Developers
 * Licensed under the Apache License, Version 2.0
 * <LICENSE-APACHE or
 * http://www.apache.org/licenses/LICENSE-2.0> or the MIT
 * license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
 * at your option. All files in the project carrying such
 * notice may not be copied, modified, or distributed except
 * according to those terms.
 */
.global _sel4_start
.global _start
.global _stack_bottom
.text

_start:
_sel4_start:
    ldr sp, =_stack_top
    /* r0, the first arg in the calling convention, is set to the bootinfo
     * pointer on startup. */
    bl __sel4_start_init_boot_info
    /* zero argc, argv */
    mov r0, #0
    mov r1, #0
    /* Now go to the "main" stub that rustc generates */
    bl main

.pool
    .data
    .align 4
    .bss
    .align  8
_stack_bottom:
    .space  65536
_stack_top:
"###
);
