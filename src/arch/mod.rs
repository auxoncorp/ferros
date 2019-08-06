#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
mod arm;
#[cfg(any(target_arch = "arm", target_arch = "aarch32"))]
pub use arm::*;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

/// For use in places where code is generated from bitfield DSL files and
/// hard-codes the output integer size rather than referring to seL4Word
/// or equivalent.
///
/// Removing this helper would involve adapting around many or all of the
/// bitfield-DSL-derived methods or replacing the generation of such.
#[cfg(target_pointer_width = "64")]
pub(crate) unsafe fn to_sel4_word(n: usize) -> u64 {
    n as u64
}

/// For use in places where code is generated from bitfield DSL files and
/// hard-codes the output integer size rather than referring to seL4Word
/// or equivalent.
///
/// Removing this helper would involve adapting around many or all of the
/// bitfield-DSL-derived methods or replacing the generation of such.
#[cfg(target_pointer_width = "32")]
pub(crate) unsafe fn to_sel4_word(n: usize) -> u32 {
    n as u32
}

#[cfg(target_pointer_width = "64")]
pub type CNodeSlotBits = typenum::U5;
#[cfg(target_pointer_width = "32")]
pub type CNodeSlotBits = typenum::U4;

type UnusedGranule = typenum::U1;
