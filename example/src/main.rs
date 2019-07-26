#![no_std]

mod error;

use ferros::*;
use ferros::alloc::*;
use error::TopLevelError;

fn main() {
    let raw_bootinfo = unsafe { &*sel4_start::BOOTINFO };

    run(raw_bootinfo).expect("Failed to run root task setup");
}

fn run(raw_bootinfo: &'static selfe_sys::seL4_BootInfo) -> Result<(), TopLevelError> {
    let (allocator, mut dev_allocator) = micro_alloc::bootstrap_allocators(&raw_bootinfo)?;
    let mut allocator = WUTBuddy::from(allocator);

    debug_println!("hello from the example!");

    Ok(())
}


#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sel4_start::debug_panic_handler(&info)
}