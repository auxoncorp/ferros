#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        sel4_sys::DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

#[macro_export]
macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}
