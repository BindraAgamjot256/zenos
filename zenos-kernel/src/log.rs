use crate::arch::serial::SerialPort;
use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};

pub use ::log::*;

const COM1_PORT: u16 = 0x3F8;

/// Monotonic sequence counter for log entries.
static LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct SerialLogger {
    port: SerialPort,
}

impl SerialLogger {
    pub const fn new(port: SerialPort) -> Self {
        Self { port }
    }

    pub fn init(&self) {
        unsafe {
            self.port.init();
        }
    }
}

impl log::Log for SerialLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Trace
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let seq = LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);

        let _ = write!(
            self.port.clone(),
            "#{:06} [{:<5}] {}: {}\n",
            seq,
            record.level(),
            record.target(),
            record.args()
        );
    }

    fn flush(&self) {}
}

pub(crate) static LOGGER: SerialLogger = SerialLogger::new(SerialPort::new(COM1_PORT));

pub fn init() {
    LOGGER.init();
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);
}
