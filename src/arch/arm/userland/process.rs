use core::cmp;
use core::mem::{self, size_of};
use core::ptr;

use selfe_sys::*;

/// Set up the target registers and stack to pass the parameter. See
/// http://infocenter.arm.com/help/topic/com.arm.doc.ihi0042f/IHI0042F_aapcs.pdf
/// "Procedure Call Standard for the ARM Architecture", Section 5.5
///
/// Returns a tuple of (regs, stack_extent), where regs only has r0-r3 set.
pub(crate) unsafe fn setup_initial_stack_and_regs(
    param: *const usize,
    param_size: usize,
    stack_top: *mut usize,
    // aarch32 does not need the child-mapped stack address.
    _: usize,
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

    // 4 words are stored in registers, so only the remainder needs to go on the
    // stack
    let param_size_on_stack =
        cmp::max(0, padded_param_size as isize - (4 * word_size) as isize) as usize;

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

    // Fill up r0 - r3 with the first 4 words.

    if p < tail as *const usize {
        // If we've got a whole word worth of data, put the whole thing in
        // the register.
        regs.r0 = *p;
        p = p.add(1);
    } else {
        // If not, store the pre-computed tail word here and be done.
        regs.r0 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r1 = *p;
        p = p.add(1);
    } else {
        regs.r1 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r2 = *p;
        p = p.add(1);
    } else {
        regs.r2 = tail_word;
        return (regs, 0);
    }

    if p < tail as *const usize {
        regs.r3 = *p;
        p = p.add(1);
    } else {
        regs.r3 = tail_word;
        return (regs, 0);
    }

    // The rest of the data goes on the stack.
    if param_size_on_stack > 0 {
        // TODO: stack pointer is supposed to be 8-byte aligned on ARM 32
        let sp = (stack_top as *mut u8).sub(param_size_on_stack);
        ptr::copy_nonoverlapping(p as *const u8, sp, param_size_on_stack);
    }

    (regs, param_size_on_stack)
}

pub(crate) fn set_thread_link_register(
    registers: &mut selfe_sys::seL4_UserContext,
    post_return_fn: fn() -> !,
) {
    registers.r14 = (post_return_fn as *const fn() -> !) as usize;
}

#[doc(hidden)]
#[allow(dead_code)]
#[cfg(feature = "test_support")]
pub mod test {
    use super::*;

    #[doc(hidden)]
    #[derive(Debug, Clone)]
    pub struct ComparisonError {
        case_name: &'static str,
        field_name: &'static str,
        expected: usize,
        actual: usize,
    }

    fn check_equal(
        case_name: &'static str,
        field_name: &'static str,
        expected: usize,
        actual: usize,
    ) -> Result<(), ComparisonError> {
        if expected != actual {
            Err(ComparisonError {
                case_name,
                field_name,
                expected,
                actual,
            })
        } else {
            Ok(())
        }
    }

    fn test_stack_setup_case<T: Sized>(
        case_name: &'static str,
        param: T,
        r0: usize,
        r1: usize,
        r2: usize,
        r3: usize,
        stack0: usize,
        sp_offset: usize,
    ) -> Result<(), ComparisonError> {
        use core::mem::size_of_val;
        let mut fake_stack = [0usize; 1024];

        let param_size = size_of_val(&param);

        let (regs, stack_extent) = unsafe {
            setup_initial_stack_and_regs(
                &param as *const T as *const usize,
                param_size,
                (&mut fake_stack[0] as *mut usize).add(1024),
                0, // unused
            )
        };

        check_equal(case_name, "r0", r0, regs.r0)?;
        check_equal(case_name, "r1", r1, regs.r1)?;
        check_equal(case_name, "r2", r2, regs.r2)?;
        check_equal(case_name, "r3", r3, regs.r3)?;
        check_equal(case_name, "top stack word", stack0, fake_stack[1023])?;
        check_equal(case_name, "sp_offset", sp_offset, stack_extent)?;

        Ok(())
    }

    #[rustfmt::skip]
    pub fn test_stack_setup() -> Result<(), ComparisonError> {
        test_stack_setup_case("single byte",
                              42u8,
                              42, 0, 0, 0, 0, 0)?;

        test_stack_setup_case("2 bytes",
                              [1u8, 2u8],
                              2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case("3 bytes",
                              [1u8, 2u8, 3u8],
                              3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case("4 bytes",
                              [1u8, 2u8, 3u8, 4u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              0, 0, 0, 0, 0)?;

        test_stack_setup_case("5 bytes",
                              [1u8, 2u8, 3u8, 4u8, 5u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              5, // r1
                              0, 0, 0, 0)?;

        test_stack_setup_case("9 bytes",
                              [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8],
                              4 << 24 | 3 << 16 | 2 << 8 | 1, // r0
                              8 << 24 | 7 << 16 | 6 << 8 | 5, // r1
                              9, // r2
                              0, 0, 0)?;

        test_stack_setup_case("13 bytes",
                              [ 1u8,  2u8,  3u8,  4u8,  5u8, 6u8, 7u8, 8u8,
                                  9u8, 10u8, 11u8, 12u8, 13u8],
                              4 << 24 |  3 << 16 |  2 << 8 |  1,  // r0
                              8 << 24 |  7 << 16 |  6 << 8 |  5,  // r1
                              12 << 24 | 11 << 16 | 10 << 8 |  9,  // r2
                              13,  // r3
                              0, 0)?;

        test_stack_setup_case("18 bytes",
                              [ 1u8,  2u8,  3u8,  4u8,  5u8,  6u8,  7u8,  8u8,
                                  9u8, 10u8, 11u8, 12u8, 13u8, 14u8, 15u8, 16u8,
                                  17u8, 18u8],
                              4 << 24 |  3 << 16 |  2 << 8 |  1,   // r0
                              8 << 24 |  7 << 16 |  6 << 8 |  5,   // r1
                              12 << 24 | 11 << 16 | 10 << 8 |  9,   // r2
                              16 << 24 | 15 << 16 | 14 << 8 | 13,   // r3
                              18 << 8 | 17,   // stack top
                              4)?; // sp offset

        Ok(())
    }

}
