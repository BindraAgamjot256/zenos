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
        unsafe { SERIAL.force_unlock() }
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

// Color codes
#[cfg(feature = "color")]
const COLOR_RESET: &str = "\x1b[0m";
#[cfg(feature = "color")]
const COLOR_ERROR: &str = "\x1b[31m"; // Red
#[cfg(feature = "color")]
const COLOR_WARN: &str = "\x1b[33m"; // Yellow
#[cfg(feature = "color")]
const COLOR_INFO: &str = "\x1b[32m"; // Green
#[cfg(feature = "color")]
const COLOR_DEBUG: &str = "\x1b[36m"; // Cyan
#[cfg(feature = "color")]
const COLOR_TRACE: &str = "\x1b[37m"; // White

#[cfg(not(feature = "color"))]
const COLOR_RESET: &str = "";
#[cfg(not(feature = "color"))]
const COLOR_ERROR: &str = "";
#[cfg(not(feature = "color"))]
const COLOR_WARN: &str = "";
#[cfg(not(feature = "color"))]
const COLOR_INFO: &str = "";
#[cfg(not(feature = "color"))]
const COLOR_DEBUG: &str = "";
#[cfg(not(feature = "color"))]
const COLOR_TRACE: &str = "";

impl log::Log for SerialLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    #[cfg(debug_assertions)]
    fn log(&self, record: &log::Record) {
        let level_color = match record.level() {
            log::Level::Error => COLOR_ERROR,
            log::Level::Warn => COLOR_WARN,
            log::Level::Info => COLOR_INFO,
            log::Level::Debug => COLOR_DEBUG,
            log::Level::Trace => COLOR_TRACE,
        };
        let reset = COLOR_RESET;
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
    #[cfg(not(debug_assertions))]
    fn log(&self, record: &log::Record) {
        use alloc::format;
        use fatfs::Write;

        let args = format!("[{}] {}", record.level(), record.args()); // remove time and color in release cuz both block the op..
        let fs = crate::disk::FS.lock();
        let mut logfile = fs.root_dir().open_file("log.log").unwrap_or_else(|e| {
            fs.root_dir()
                .create_file("log.log")
                .expect("failed to create log.log");
            fs.root_dir()
                .open_file("log.log")
                .expect("failed to open log.log")
        });
        logfile
            .write(args.as_bytes())
            .expect("failed to write to log.log");
    }

    fn flush(&self) {}
}
