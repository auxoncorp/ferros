#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
mod arm;
#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
pub use arm::*;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

#[cfg(target_pointer_width = "64")]
pub(crate) unsafe fn to_sel4_word(n: usize) -> u64 {
    n as u64
}

#[cfg(target_pointer_width = "32")]
pub(crate) unsafe fn to_sel4_word(n: usize) -> u32 {
    n as u32
}
