use core::cmp;
use core::mem::{self, size_of};
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
    let word_size = size_of::<usize>();

    // The 'tail' is the part of the parameter that doesn't fit in the
    // word-aligned part.
    let tail_size = param_size % word_size;

    // The parameter must be zero-padded, at the end, to a word boundary
    let padding_size = if tail_size == 0 {
        0
    } else {
        word_size - tail_size
    };
    let padded_param_size = param_size + padding_size;

    // 8 words are stored in registers, so only the remainder needs to go on the
    // stack
    let param_size_on_stack =
        cmp::max(0, padded_param_size as isize - (8 * word_size) as isize) as usize;

    let mut regs: seL4_UserContext = mem::zeroed();

    // The cursor pointer to traverse the parameter data word one word at a
    // time
    let mut p = param;

    // This is the pointer to the start of the tail.
    let tail = (p as *const u8).add(param_size).sub(tail_size);

    // Compute the tail word ahead of time, for easy use below.
    let mut tail_word = 0usize;
    if tail_size >= 1 {
        tail_word |= *tail.add(0) as usize;
    }

    if tail_size >= 2 {
        tail_word |= (*tail.add(1) as usize) << 8;
    }

    if tail_size >= 3 {
        tail_word |= (*tail.add(2) as usize) << 16;
    }

    if tail_size >= 4 {
        tail_word |= (*tail.add(3) as usize) << 24;
    }

    if tail_size >= 5 {
        tail_word |= (*tail.add(4) as usize) << 32;
    }

    if tail_size >= 6 {
        tail_word |= (*tail.add(5) as usize) << 40;
    }

    if tail_size >= 7 {
        tail_word |= (*tail.add(6) as usize) << 48;
    }

    // Fill up x0 - r7 with the first 8 words.

    if p < tail as *const usize {
        // If we've got a whole word worth of data, put the whole thing in
        // the register.
        regs.x0 = *p;
        p = p.add(1);
    } else {
        // If not, store the pre-computed tail word here and be done.
        regs.x0 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x1 = *p;
        p = p.add(1);
    } else {
        regs.x1 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x2 = *p;
        p = p.add(1);
    } else {
        regs.x2 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x3 = *p;
        p = p.add(1);
    } else {
        regs.x3 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x4 = *p;
        p = p.add(1);
    } else {
        regs.x4 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x5 = *p;
        p = p.add(1);
    } else {
        regs.x5 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x6 = *p;
        p = p.add(1);
    } else {
        regs.x6 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.x7 = *p;
        p = p.add(1);
    } else {
        regs.x7 = tail_word;
        return (regs, 0);
    }

    // The rest of the data goes on the stack.
    if param_size_on_stack > 0 {
        // TODO: stack pointer is supposed to be aligned somehow
        let sp = (stack_top as *mut u8).sub(param_size_on_stack);
        ptr::copy_nonoverlapping(p as *const u8, sp, param_size_on_stack);
    }

    (regs, param_size_on_stack)
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
