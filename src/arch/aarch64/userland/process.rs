use core::cmp;
use core::mem;
use core::ptr;

use selfe_sys::*;

/// Set up the target registers and stack to pass the parameter.
/// https://en.wikipedia.org/wiki/Calling_convention#ARM_(A64)
///
/// Returns a tuple of (regs, stack_extent), where regs only has x0-x7 set.
pub(crate) unsafe fn setup_initial_stack_and_regs(
    param: *const usize,
    param_size: usize,
    stack_top: *mut usize,
) -> (seL4_UserContext, usize) {
    let word_size = mem::size_of::<usize>();
    let alignment_gap = param_size % word_size;
    let padded_param_size = param_size + alignment_gap;
    let mut regs: seL4_UserContext = mem::zeroed();

    if alignment_gap != 0 {
        let mut ptr = param as *mut u8;
        ptr = ptr.add(param_size);
        for _ in 0..alignment_gap {
            ptr::write_volatile(ptr, 0_u8);
            ptr = ptr.add(1);
        }
    }

    if padded_param_size >= word_size * 8 {
        let mut ptr = param;
        regs.x0 = *ptr;
        ptr = ptr.add(1);

        if ptr as usize != padded_param_size {
            regs.x1 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x2 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x3 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x4 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x5 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x6 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }

        if ptr as usize != padded_param_size {
            regs.x7 = *ptr;
            ptr = ptr.add(1);
        } else {
            return (regs, padded_param_size);
        }
        return (regs, padded_param_size);
    } else {
        let mut ptr = param as *const u8;
        let mut stack_ptr = stack_top as *mut u8;
        for offset in 0..padded_param_size {
            ptr::write_volatile(stack_ptr, *ptr);
            ptr = ptr.add(1);
            stack_ptr = stack_ptr.sub(1);
        }
        return (regs, padded_param_size);
    }
}

pub(crate) fn set_thread_link_register(
    registers: &mut selfe_sys::seL4_UserContext,
    post_return_fn: fn() -> !,
) {
    registers.x30 = (post_return_fn as *const fn() -> !) as usize;
}

#[doc(hidden)]
#[allow(dead_code)]
#[cfg(feature = "test_support")]
pub mod test {
    use super::*;

    #[doc(hidden)]
    #[derive(Debug, Clone)]
    pub struct ComparisonError {
        name: &'static str,
        expected: usize,
        actual: usize,
    }
    #[rustfmt::skip]
    pub fn test_stack_setup() -> Result<(), ComparisonError> {
        Err(ComparisonError {
            name: "No comparisons have been implemented",
            expected: 3141592653589,
            actual: 0
        })
    }
}
