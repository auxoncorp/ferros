use crate::userland;

pub struct Params {
    pub nums: [usize; 140],
}

impl userland::RetypeForSetup for Params {
    type Output = Params;
}

// 'extern' to force C calling conventions
pub extern "C" fn main(params: &Params) {
    debug_println!("");
    debug_println!("*** Hello from a feL4 process!");
    for i in params.nums.iter() {
        debug_println!("  {:08x}", i);
    }

    debug_println!("");
}
