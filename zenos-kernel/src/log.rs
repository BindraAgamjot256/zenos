use arch_common::serial::SerialPort;
use core::fmt::Write;

pub use ::log::*;

const COM1_PORT: u16 = 0x3F8;

pub struct SerialLogger{
    port: SerialPort,
}

impl SerialLogger{
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
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        write!(self.port.clone(), "[{}] - {}\n", record.level(), record.args()).unwrap();
    }

    fn flush(&self) {}
}


pub(crate) static LOGGER: SerialLogger = SerialLogger::new(SerialPort::new(COM1_PORT));

pub fn init() {
    LOGGER.init();
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
}