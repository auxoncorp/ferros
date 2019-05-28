#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
mod arm;
#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
pub use arm::*;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
