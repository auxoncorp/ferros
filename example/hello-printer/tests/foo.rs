#![no_std]
#![no_main]

#![feature(custom_test_frameworks)]
#![test_runner(fancy_test::runner)]
#![reexport_test_harness_main = "test_main"]

extern crate selfe_runtime;

#[cfg(test)]
#[no_mangle]
pub unsafe extern "C" fn _start() {
    test_main();

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}

use fancy_test::UnitTest;

#[test_case]
const integration_pass: UnitTest = UnitTest {
    name: "integration_pass",
    f: || {
        assert_eq!(1, 1);
    }
};

#[test_case]
const fail: UnitTest = UnitTest {
    name: "integration_fail",
    f: || {
        assert_eq!(1, 2);
    }
};


