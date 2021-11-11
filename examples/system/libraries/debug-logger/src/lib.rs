#![no_std]

use ferros::debug_println;
use log::{LevelFilter, Metadata, Record};

pub struct DebugLogger;

impl DebugLogger {
    /// Behaves like env-logger RUST_LOG, but at compile time
    pub fn max_log_level_from_env() -> LevelFilter {
        match option_env!("RUST_LOG") {
            Some("off") => LevelFilter::Off,
            Some("error") => LevelFilter::Error,
            Some("warn") => LevelFilter::Warn,
            Some("info") => LevelFilter::Info,
            Some("debug") => LevelFilter::Debug,
            Some("trace") => LevelFilter::Trace,
            _ => LevelFilter::Debug,
        }
    }
}

impl log::Log for DebugLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level().to_level_filter() <= Self::max_log_level_from_env()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            debug_println!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
