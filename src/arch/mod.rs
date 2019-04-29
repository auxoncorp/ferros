// #[cfg(target_arch="arm32")]
// or
// #[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
pub mod arm32;

// #[cfg(target_arch="arm32")]
// or
// #[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
pub use self::arm32::*;
