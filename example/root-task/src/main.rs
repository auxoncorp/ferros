#![no_std]

mod error;

use ferros::*;
use ferros::alloc::*;
use error::TopLevelError;
use typenum::*;
use selfe_arc;

use hello_printer;

extern "C" {
    static _selfe_arc_data_start: u8;
    static _selfe_arc_data_end: usize;
}

trait ElfProc: Sized {
    // The name of the image in the selfe_arc
    const IMAGE_NAME: &'static str;

    // The total number of pages which need to be mapped when starting the process, as a typenum.
    type RequiredPages: Unsigned;

    // The number of pages which need to be mapped as writeable (data and BSS sections), as a typenum.
    type WritablePages: Unsigned;
}

// TODO codegen this module
mod elf_images {
    use super::ElfProc;
    use typenum::*;

    pub struct HelloPrinter {}
    impl ElfProc for HelloPrinter {
        const IMAGE_NAME: &'static str = "hello-printer";
        type RequiredPages = U128;
        type WritablePages = U16;
    }
}


fn main() {
    let raw_bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(raw_bootinfo).expect("Failed to run root task setup");
}

fn run(raw_bootinfo: &'static selfe_sys::seL4_BootInfo) -> Result<(), TopLevelError> {
    let (allocator, mut dev_allocator) = micro_alloc::bootstrap_allocators(&raw_bootinfo)?;
    let mut allocator = WUTBuddy::from(allocator);

    debug_println!("\n\n\n    Hello from the root task!\n\n\n");

    let archive_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &_selfe_arc_data_start,
            &_selfe_arc_data_end as *const _ as usize - &_selfe_arc_data_start as *const _ as usize,
        )
    };

    let archive = selfe_arc::read::Archive::from_slice(archive_slice);
    let proc_file_slice = archive.file(elf_images::HelloPrinter::IMAGE_NAME).expect("find hello-printer in arc");

    debug_println!("Binary found, size is {}", proc_file_slice.len());
    debug_println!("\n\n\n");
    Ok(())
}

