#![no_std]
#![no_main]

#![feature(custom_test_frameworks)]
#![test_runner(fancy_test::runner)]
#![reexport_test_harness_main = "test_main"]

use ferros::*;
use ferros::cap::*;
extern crate selfe_runtime;

use hello_printer::ProcParams;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start(params: ProcParams) -> ! {
    debug_println!("hello from elf!");

    for i in 0..params.number_of_hellos {
        debug_println!("Hello elven world {}!", i);
    }

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }
}

#[cfg(test)]
#[no_mangle]
pub extern "C" fn _start(context: fancy_test::TestContext<role::Local>) {
    fancy_test::set_test_context(context);
    test_main();
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
        name: "pass2",
        f: || {
            assert_eq!(2, 2);
        },
    };
}
