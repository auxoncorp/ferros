#![no_std]
#![no_main]

use ferros::*;
extern crate selfe_runtime;

use hello_printer::ProcParams;

#[no_mangle]
pub extern "C" fn _start(params: ProcParams) {
    for i in 0..params.number_of_hellos {
        debug_println!("Hello, world {}!", i);
    }
}
