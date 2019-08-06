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
    child_stack_top: usize,
) -> (seL4_UserContext, usize) {
    let word_size = mem::size_of::<usize>();
    let tail_size = param_size % word_size;
    let padding_size = if tail_size == 0 {
        0
    } else {
        word_size - tail_size
    };
    let padded_param_size = param_size + padding_size;

    let mut regs: seL4_UserContext = mem::zeroed();

    if padded_param_size <= 16 {
        let mut p = param;
        let tail = (param as *const u8).add(param_size).sub(tail_size);

        let mut tail_word = 0_usize;
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
        } else {
            regs.x1 = tail_word;
        }
        return (regs, 0);
    } else {
        let sp = (stack_top as *mut u8).sub(param_size);
        ptr::copy_nonoverlapping(param as *const u8, sp, param_size);
        regs.x0 = child_stack_top - param_size;
    }

    (regs, param_size)
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

    fn smaller_than_16() -> Result<(), ComparisonError> {
        let smaller_than_16: [usize; 1] = [42; 1];
        let mut stack: [usize; 256] = [0; 256];
        let stack_top = unsafe { (&mut stack as *mut [usize; 256] as *mut usize).add(256) };
        let child_stack_top = 2048;
        let (regs, param_size) = unsafe {
            setup_initial_stack_and_regs(
                &smaller_than_16 as *const usize,
                mem::size_of::<[usize; 1]>(),
                stack_top,
                child_stack_top,
            )
        };
        if param_size != 0 {
            return Err(ComparisonError {
                name: "smaller_than_16: param size was incorrect",
                expected: 0,
                actual: param_size,
            });
        }

        if regs.x0 != 42 {
            return Err(ComparisonError {
                name: "smaller_than_16: x0 was incorrect",
                expected: 42,
                actual: regs.x0,
            });
        }
        Ok(())
    }

    fn is_16() -> Result<(), ComparisonError> {
        let is_16: [usize; 2] = [42; 2];
        let mut stack: [usize; 256] = [0; 256];
        let stack_top = unsafe { (&mut stack as *mut [usize; 256] as *mut usize).add(256) };
        let child_stack_top = 2048;
        let (regs, param_size) = unsafe {
            setup_initial_stack_and_regs(
                &is_16 as *const usize,
                mem::size_of::<[usize; 2]>(),
                stack_top,
                child_stack_top,
            )
        };
        if param_size != 0 {
            return Err(ComparisonError {
                name: "is_16: param size was incorrect",
                expected: 0,
                actual: param_size,
            });
        }

        if regs.x0 != 42 {
            return Err(ComparisonError {
                name: "is_16: x0 was incorrect",
                expected: 42,
                actual: regs.x0,
            });
        }
        if regs.x1 != 42 {
            return Err(ComparisonError {
                name: "is_16: x1 was incorrect",
                expected: 42,
                actual: regs.x1,
            });
        }
        Ok(())
    }

    fn larger_than_16() -> Result<(), ComparisonError> {
        let larger_than_16: [usize; 10] = [42; 10];
        let mut stack: [usize; 256] = [0; 256];
        let stack_top = unsafe { (&mut stack as *mut [usize; 256] as *mut usize).add(256) };
        let child_stack_top = 2048;
        let (regs, param_size) = unsafe {
            setup_initial_stack_and_regs(
                &larger_than_16 as *const usize,
                mem::size_of::<[usize; 10]>(),
                stack_top,
                child_stack_top,
            )
        };
        if param_size != 80 {
            return Err(ComparisonError {
                name: "larger_than_16: param size was incorrect",
                expected: 80,
                actual: param_size,
            });
        }

        if regs.x0 != child_stack_top - mem::size_of::<[usize; 10]>() {
            return Err(ComparisonError {
                name: "larger_than_16: x0 was incorrect",
                expected: mem::size_of::<[usize; 10]>(),
                actual: regs.x0,
            });
        }
        for idx in 0..10 {
            // we should copy into the last ten slots of the stack.
            if stack[idx + 246] != 42 {
                return Err(ComparisonError {
                    name: "larger_than_16: stack was incorrect",
                    expected: 42,
                    actual: stack[idx],
                });
            }
        }
        Ok(())
    }

    pub fn test_stack_setup() -> Result<(), ComparisonError> {
        smaller_than_16()?;
        is_16()?;
        larger_than_16()?;
        Ok(())
    }
}
