// NOTE: this file is generated by fel4
// NOTE: Don't edit it here; your changes will be lost at the next build!
#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]
#![feature(lang_items, core_intrinsics)]
#![feature(global_asm)]
#![cfg_attr(feature = "alloc", feature(global_allocator))]
#![feature(panic_info_message)]

#[cfg(feature = "alloc")]
extern crate alloc;
extern crate sel4_sys;
#[cfg(feature = "alloc")]
extern crate wee_alloc;
#[cfg(all(feature = "test", feature = "alloc"))]
#[macro_use]
extern crate proptest;
extern crate ferros;

use core::alloc::Layout;
use core::intrinsics;
use core::panic::PanicInfo;
use sel4_sys::*;

#[cfg(feature = "alloc")]
#[global_allocator]
static ALLOCATOR: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// include the seL4 kernel configurations
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
pub mod sel4_config {
    pub const BuildWithCommonSimulationSettings: bool = true;
    pub const ElfloaderErrata764369: bool = true;
    pub const ElfloaderImage: &'static str = "elf";
    pub const ElfloaderMode: &'static str = "secure supervisor";
    pub const HardwareDebugAPI: bool = false;
    pub const KernelAArch32FPUEnableContextSwitch: bool = true;
    pub const KernelARMPlatform: &'static str = "sabre";
    pub const KernelArch: &'static str = "arm";
    pub const KernelArmEnableA9Prefetcher: bool = false;
    pub const KernelArmExportPMUUser: bool = false;
    pub const KernelArmSel4Arch: &'static str = "aarch32";
    pub const KernelBenchmarks: &'static str = "none";
    pub const KernelColourPrinting: bool = true;
    pub const KernelDebugBuild: bool = true;
    pub const KernelDebugDisableBranchPrediction: bool = false;
    pub const KernelDebugDisableL2Cache: bool = false;
    pub const KernelFPUMaxRestoresSinceSwitch: &'static str = "64";
    pub const KernelFWholeProgram: bool = false;
    pub const KernelFastpath: bool = true;
    pub const KernelIPCBufferLocation: &'static str = "threadID_register";
    pub const KernelMaxNumBootinfoUntypedCaps: &'static str = "230";
    pub const KernelMaxNumNodes: &'static str = "1";
    pub const KernelMaxNumWorkUnitsPerPreemption: &'static str = "100";
    pub const KernelNumDomains: &'static str = "1";
    pub const KernelNumPriorities: &'static str = "256";
    pub const KernelOptimisation: &'static str = "-O2";
    pub const KernelPrinting: bool = true;
    pub const KernelResetChunkBits: &'static str = "8";
    pub const KernelRetypeFanOutLimit: &'static str = "16384";
    pub const KernelRootCNodeSizeBits: &'static str = "19";
    pub const KernelStackBits: &'static str = "12";
    pub const KernelTimeSlice: &'static str = "5";
    pub const KernelTimerTickMS: &'static str = "2";
    pub const KernelUserStackTraceLength: &'static str = "16";
    pub const KernelVerificationBuild: bool = false;
    pub const LibSel4DebugAllocBufferEntries: &'static str = "0";
    pub const LibSel4DebugFunctionInstrumentation: &'static str = "none";
    pub const LibSel4FunctionAttributes: &'static str = "public";
    pub const LinkPageSize: &'static str = "4096";
    pub const UserLinkerGCSections: bool = false;
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
#[no_mangle]
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
    unsafe { core::intrinsics::abort() }
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

fn main() {
    let boot_info = unsafe { &*BOOTINFO };
    #[cfg(feature = "test")]
    {
        ferros::fel4_test::run(boot_info);
    }
    #[cfg(not(feature = "test"))]
    {
        ferros::run(boot_info);
    }
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
    .space  2097152
_stack_top:
"###
);
