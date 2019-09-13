#![no_std]
#![no_main]

#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(fancy_test::runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

use ferros::*;
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
pub extern "C" fn _start(context: fancy_test::TestContext<ferros::cap::role::Local>) {
    fancy_test::set_test_context(context);
    test_main();
}

#[cfg(test)]
mod test {
    use fancy_test::unit_test;

    #[unit_test]
    fn pass() {
        assert_eq!(1, 1);
    }

    #[unit_test]
    fn pass2() {
        assert_eq!(2, 2);
    }
}
