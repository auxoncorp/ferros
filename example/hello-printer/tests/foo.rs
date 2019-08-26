#![no_std]
#![no_main]

#![feature(custom_test_frameworks)]
#![test_runner(fancy_test::runner)]

extern crate selfe_runtime;
use ferros::*;
use ferros::cap::*;

#[no_mangle]
pub extern "C" fn _start(context: fancy_test::TestContext<role::Local>) -> ! {
    assert_eq!(1, 1);

    context.complete()
}
