use crate::arch::serial::SerialPort;
use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};

pub use ::log::*;

const COM1_PORT: u16 = 0x3F8;
#[cfg(feature = "__test_timer")]
const QMP_PAUSE_MARKER: &[u8] = &[0xFF, 0xFF, 0x00, 0x00];
#[cfg(feature = "__test_timer")]
const QMP_COMPLETE_MARKER: &[u8] = &[0xFF, 0xFF, 0x00, 0x01];
#[cfg(feature = "__test_timer")]
const QMP_PAUSE_ACK: u8 = 0xAC;
#[cfg(feature = "__test_timer")]
const QMP_COMPLETE_ACK: u8 = 0xAD;

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

        let _ = writeln!(
            self.port.clone(),
            "#{:06} [{:<5}] {}: {}",
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

#[cfg(feature = "__test_timer")]
fn write_raw(bytes: &[u8]) {
    let port = LOGGER.port;
    for &byte in bytes {
        port.write_byte(byte);
    }
}

#[cfg(feature = "__test_timer")]
pub fn qmp_pause_barrier() {
    write_raw(QMP_PAUSE_MARKER);

    while LOGGER.port.read_byte() != QMP_PAUSE_ACK {}
}

#[cfg(feature = "__test_timer")]
pub fn qmp_pause_complete() {
    write_raw(QMP_COMPLETE_MARKER);

    while LOGGER.port.read_byte() != QMP_COMPLETE_ACK {}
}
