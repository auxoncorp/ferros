use super::TopLevelError;
use selfe_sys::seL4_BootInfo;
pub fn run(_raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    debug_println!("\nhello from the root task!\n");
    Ok(())
}
