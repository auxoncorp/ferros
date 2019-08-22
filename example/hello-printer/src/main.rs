#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(fancy_test::runner)]
#![reexport_test_harness_main = "test_main"]

extern crate selfe_runtime;

#[cfg(test)]
#[no_mangle]
pub unsafe extern "C" fn _start(params: TestProcParams) {
    fancy_test::set_test_proc_params(params);
    test_main();

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}

#[cfg(test)]
mod test {
    use fancy_test::UnitTest;

    #[test_case]
    const pass: UnitTest = UnitTest {
        name: "pass",
        f: || {
            assert_eq!(1, 1);
        },
    };

    #[test_case]
    const fail: UnitTest = UnitTest {
        name: "fail",
        f: || {
            assert_eq!(1, 2);
        },
    };
}
