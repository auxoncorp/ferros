use crate::fancy;

#[cfg(feature = "KernelPrinting")]
use sel4_sys::DebugOutHandle;

macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}

pub struct Params {
    pub nums: [usize; 140],
}

impl fancy::RetypeForSetup for Params {
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
