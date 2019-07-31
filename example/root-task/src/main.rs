#![no_std]
#![feature(proc_macro_hygiene)]

mod error;

use error::TopLevelError;
use ferros::alloc::*;
use ferros::bootstrap::*;
use ferros::cap::*;
use ferros::userland::*;
use ferros::vspace::ElfProc;
use ferros::vspace::*;
use ferros::*;
use selfe_arc;
use typenum::*;
use xmas_elf;

use hello_printer;

extern "C" {
    static _selfe_arc_data_start: u8;
    static _selfe_arc_data_end: usize;
}

// TODO codegen this module
mod elf_images {
    // use super::ElfProc;
    use ferros::vspace::ElfProc;
    use typenum::*;

    pub struct HelloPrinter {}
    impl ElfProc for HelloPrinter {
        const IMAGE_NAME: &'static str = "hello-printer";
        type RequiredPages = U128;
        type WritablePages = U3;
        type RequiredMemoryBits = U14;
        type StackSizeBits = U12;
    }
}

fn main() {
    let raw_bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(raw_bootinfo).expect("Failed to run root task setup");
}

fn run(raw_bootinfo: &'static selfe_sys::seL4_BootInfo) -> Result<(), TopLevelError> {
    let (allocator, mut dev_allocator) = micro_alloc::bootstrap_allocators(&raw_bootinfo)?;
    let mut allocator = WUTBuddy::from(allocator);

    let (root_cnode, local_slots) = root_cnode(&raw_bootinfo);
    let (root_vspace_slots, local_slots): (LocalCNodeSlots<U100>, _) = local_slots.alloc();
    let (ut_slots, local_slots): (LocalCNodeSlots<U100>, _) = local_slots.alloc();
    let mut ut_slots = ut_slots.weaken();

    let BootInfo {
        mut root_vspace,
        asid_control,
        user_image,
        root_tcb,
        ..
    } = BootInfo::wrap(
        &raw_bootinfo,
        allocator.alloc_strong::<U16>(&mut ut_slots)?,
        root_vspace_slots,
    );

    let tpa = root_tcb.downgrade_to_thread_priority_authority();

    let archive_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &_selfe_arc_data_start,
            &_selfe_arc_data_end as *const _ as usize - &_selfe_arc_data_start as *const _ as usize,
        )
    };

    let archive = selfe_arc::read::Archive::from_slice(archive_slice);
    let hello_printer_elf_data = archive
        .file(elf_images::HelloPrinter::IMAGE_NAME)
        .expect("find hello-printer in arc");

    debug_println!("Binary found, size is {}", hello_printer_elf_data.len());
    debug_println!("\n\n\n");

    let elf = xmas_elf::ElfFile::new(hello_printer_elf_data).expect("Error parsing elf file");
    let entry_point = elf.header.pt2.entry_point() as usize;

    let uts = alloc::ut_buddy(allocator.alloc_strong::<U20>(&mut ut_slots)?);
    smart_alloc!(|slots: local_slots, ut: uts| {
        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (hello_asid, asid_pool) = asid_pool.alloc();

        // TODO: can we determine these numbers statically now, from the elf file's layout?
        let vspace_slots: LocalCNodeSlots<U16> = slots;
        let vspace_ut: LocalCap<Untyped<U16>> = ut;

        let mut hello_vspace = VSpace::new_from_elf::<elf_images::HelloPrinter>(
            retype(ut, slots)?, // paging_root
            hello_asid,
            vspace_slots.weaken(), // slots
            vspace_ut.weaken(),    // paging_untyped
            &hello_printer_elf_data,
            slots, // page_slots
            ut,    // elf_writable_mem
            &user_image,
            &root_cnode,
            &mut root_vspace,
        )?;

        let (hello_cnode, hello_slots) = retype_cnode::<U12>(ut, slots)?;
        let params = hello_printer::ProcParams {
            number_of_hellos: 5,
            data: [0xab; 124],
        };

        let stack_mem: UnmappedMemoryRegion<
            <elf_images::HelloPrinter as ElfProc>::StackSizeBits,
            _,
        > = UnmappedMemoryRegion::new(ut, slots).unwrap();
        let stack_mem =
            root_vspace.map_region(stack_mem, CapRights::RW, arch::vm_attributes::DEFAULT)?;

        let hello_process = StandardProcess::new::<hello_printer::ProcParams>(
            &mut hello_vspace,
            hello_cnode,
            stack_mem,
            &root_cnode,
            unsafe { core::mem::transmute(entry_point) }, // proc_main
            params,
            ut,
            ut,
            slots,
            &tpa,
            None, // fault
        )?;
    });

    hello_process.start()?;

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}
