use proptest::test_runner::{TestCaseError, TestError, TestRunner};

use core::fmt;
#[cfg(feature = "KernelPrinting")]
use sel4_sys::DebugOutHandle;
use sel4_sys::*;

#[cfg(feature = "KernelPrinting")]
macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

#[cfg(feature = "KernelPrinting")]
macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}

#[cfg(feature = "KernelPrinting")]
pub fn run() {
    debug_println!("\n\nrunning example tests");
    let mut runner = TestRunner::default();
    let mut num_passed = 0;
    let mut num_failed = 0;
    for found_success in [
        print_test_result(
            "test_message_info_predictability",
            test_message_info_predictability(&mut runner),
        ),
        print_test_result(
            "test_cap_rights_predictability",
            test_cap_rights_predictability(&mut runner),
        ),
    ].iter()
        {
            if *found_success {
                num_passed += 1;
            } else {
                num_failed += 1;
            }
        }
    debug_println!(
        "test result: {}. {} passed; {} failed\n\n",
        if num_failed == 0 { "ok" } else { "FAILED" },
        num_passed,
        num_failed
    );
    halt();
}

fn test_message_info_predictability(
    runner: &mut TestRunner,
) -> Result<(), TestError<(u32, u32, u32, u32)>> {
    runner.run(
        &(0u32..0xfffff, 0u32..0x7, 0u32..0x3, 0u32..0x7f),
        |input| {
            let (label, caps, extra, length) = input;
            let (label, caps, extra, length) = (label as seL4_Word, caps as seL4_Word, extra as seL4_Word, length as seL4_Word);
            let out = unsafe {
                let msg = seL4_MessageInfo_new(label, caps, extra, length);
                let ptr = &msg as *const seL4_MessageInfo_t as *mut seL4_MessageInfo_t;
                (
                    seL4_MessageInfo_ptr_get_label(ptr),
                    seL4_MessageInfo_ptr_get_capsUnwrapped(ptr),
                    seL4_MessageInfo_ptr_get_extraCaps(ptr),
                    seL4_MessageInfo_ptr_get_length(ptr),
                )
            };
            let (out_label, out_caps, out_extra, out_length) = out;
            if label == out_label && caps == out_caps && extra == out_extra && length == out_length
                {
                    Ok(())
                } else {
                Err(TestCaseError::fail(format!(
                    "Mismatched input and output. {:?} vs {:?}",
                    input, &out
                )))
            }
        },
    )
}

fn test_cap_rights_predictability(
    runner: &mut TestRunner,
) -> Result<(), TestError<(u32, u32, u32)>> {
    runner.run(&(0u32..2, 0u32..2, 0u32..2), |input| {
        let (grant, read, write) = input;
        let (grant, read, write) = (grant as seL4_Word, read as seL4_Word, write as seL4_Word);
        let out = unsafe {
            let msg = seL4_CapRights_new(grant, read, write);
            let ptr = &msg as *const seL4_CapRights_t as *mut seL4_CapRights_t;
            (
                seL4_CapRights_ptr_get_capAllowRead(ptr),
                seL4_CapRights_ptr_get_capAllowGrant(ptr),
                seL4_CapRights_ptr_get_capAllowWrite(ptr),
            )
        };
        let (out_grant, out_read, out_write) = out;
        if grant == out_grant && read == out_read && write == out_write {
            Ok(())
        } else {
            Err(TestCaseError::fail(format!(
                "Mismatched input and output. {:?} vs {:?}",
                input, &out
            )))
        }
    })
}

/// Prints a summary of the test output.
/// Returns true if the test succeeded, false otherwise.
fn print_test_result<T: fmt::Debug>(
    test_name: &'static str,
    result: Result<(), TestError<T>>,
) -> bool {
    match result {
        Ok(_) => {
            debug_println!("{} ... ok", test_name);
            true
        }
        Err(e) => {
            debug_println!("{} ... FAILED\n\t{}", test_name, e);
            false
        }
    }
}

#[cfg(all(feature = "KernelDebugBuild", not(feature = "KernelPrinting")))]
pub fn run() {
    halt();
}

#[cfg(feature = "KernelDebugBuild")]
fn halt() {
    unsafe { seL4_DebugHalt() };
}
#[cfg(not(feature = "KernelDebugBuild"))]
fn halt() {
    panic!("Halting");
}
