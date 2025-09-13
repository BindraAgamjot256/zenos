use spin::{Lazy, Mutex};
use uart_16550::SerialPort;

/// Initializes the serial port at the specified base address.
pub static SERIAL: Lazy<Mutex<SerialPort>> = Lazy::new(|| {
    let mut serial_port = unsafe { SerialPort::new(0x3F8) }; // COM1 port
    serial_port.init();
    Mutex::new(serial_port)
});

/// A global logger that writes log messages to the serial port.
pub static LOGGER: SerialLogger = SerialLogger;

#[doc(hidden)]
pub fn _sprint(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts::without_interrupts;
    without_interrupts(|| {
        let mut serial = SERIAL.lock();
        serial.write_fmt(args).unwrap();
    })
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_sprint(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}

/// A logger that writes log messages to the serial port.
pub struct SerialLogger;
impl log::Log for SerialLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        // Filter out trace logs from specific modules
        if record.level() == log::Level::Trace {
            if let Some(module) = record.module_path() {
                if module.starts_with("zenos_kernel::memory::alloc")
                    || module.starts_with("zenos_kernel::hardware::")
                {
                    return; // skip these logs
                }
            }
        }

        let level_color = match record.level() {
            log::Level::Error => "\x1b[31m", // Red
            log::Level::Warn => "\x1b[33m",  // Yellow
            log::Level::Info => "\x1b[32m",  // Green
            log::Level::Debug => "\x1b[34m", // Blue
            log::Level::Trace => "\x1b[35m", // Magenta
        };
        let reset = "\x1b[0m";

        serial_println!(
            "{}[{}: {}: {}]{}: {}",
            level_color,
            crate::time::RtcTime::read(),
            record.module_path().unwrap_or("unknown"),
            record.level(),
            reset,
            record.args()
        );
    }

    fn flush(&self) {}
}
