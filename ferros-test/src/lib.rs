#![no_std]
#![allow(dead_code)]

pub use test_macro_impl::ferros_test;

#[cfg(feature = "sel4_start_main")]
#[doc(hidden)]
pub fn sel4_start_main(tests: &[&ferros::test_support::RunTest]) {
    let raw_boot_info = unsafe { &*sel4_start::BOOTINFO };
    let allocator = ferros::alloc::micro_alloc::Allocator::bootstrap(raw_boot_info)
        .expect("Test allocator setup failure");
    let (mut resources, reporter) =
        ferros::test_support::Resources::with_debug_reporting(raw_boot_info, allocator)
            .expect("Test resource setup failure");

    ferros::test_support::execute_tests(reporter, resources.as_mut_ref(), tests)
        .expect("Test execution failure");

    let suspend_error =
        unsafe { ::selfe_sys::seL4_TCB_Suspend(::selfe_sys::seL4_CapInitThreadTCB as usize) };
    if suspend_error != 0 {
        use core::fmt::Write;
        writeln!(
            sel4_start::DebugOutHandle,
            "Error suspending root task thread: {}",
            suspend_error
        )
        .unwrap();
    }
}

#[cfg(feature = "sel4_start_main")]
#[doc(hidden)]
pub fn sel4_start_panic_handler(info: &core::panic::PanicInfo) -> ! {
    sel4_start::debug_panic_handler(&info)
}

#[cfg(feature = "sel4_start_main")]
#[macro_export]
macro_rules! ferros_test_main {
    ($tests:expr) => {
        fn main() {
            $crate::sel4_start_main($tests)
        }

        #[panic_handler]
        fn panic(info: &core::panic::PanicInfo) -> ! {
            $crate::sel4_start_panic_handler(info)
        }
    };
}
