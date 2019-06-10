use core::cmp;
use core::mem::{self, size_of};
use core::ptr;

use selfe_sys::*;

/// Set up the target registers and stack to pass the parameter.
/// https://en.wikipedia.org/wiki/Calling_convention#ARM_(A64)
///
/// Returns a tuple of (regs, stack_extent), where regs only has x0-x7 set.
pub(crate) unsafe fn setup_initial_stack_and_regs(
    _param: *const usize,
    _param_size: usize,
    _stack_top: *mut usize,
) -> (seL4_UserContext, usize) {
    // TODO - likely can borrow from the arm32 implementation, with slight adjustment
    // for the extra registers available (x0-x7 instead of r0-r3)
    unimplemented!()
}
