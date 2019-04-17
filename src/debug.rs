use core::fmt;

pub struct DebugOutHandle;

impl fmt::Write for DebugOutHandle {
    #[cfg(KernelPrinting)]
    fn write_str(&mut self, s: &str) -> ::core::fmt::Result {
        for &b in s.as_bytes() {
            unsafe { sel4_sys::seL4_DebugPutChar(b as i8) };
        }
        Ok(())
    }

    #[cfg(not(KernelPrinting))]
    fn write_str(&mut self, _s: &str) -> ::core::fmt::Result {
        Ok(())
    }
}

#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        $crate::debug::DebugOutHandle.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

#[macro_export]
macro_rules! debug_println {
    ($fmt:expr) => (debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (debug_print!(concat!($fmt, "\n"), $($arg)*));
}
