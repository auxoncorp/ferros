/// The classic no-op
#[inline(always)]
pub fn nop() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
    unsafe {
        asm!("nop", options(nomem, nostack))
    }
}
