use crate::serial::SerialPort;
use conquer_once::spin::OnceCell;
use core::fmt::Write;
use spinning_top::Spinlock;

/// The global logger instance used for the `log` crate.
pub static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();

/// A logger instance protected by a spinlock.
pub struct LockedLogger {
    serial: Option<Spinlock<SerialPort>>,
}

impl LockedLogger {
    /// Create a new instance that logs to the given framebuffer.
    pub fn new(serial_logger_status: bool) -> Self {
        let serial = match serial_logger_status {
            true => Some(Spinlock::new(unsafe { SerialPort::init() })),
            false => None,
        };

        LockedLogger { serial }
    }

    /// Force-unlocks the logger to prevent a deadlock.
    ///
    /// ## Safety
    /// This method is not memory safe and should be only used when absolutely necessary.
    pub unsafe fn force_unlock(&self) {
        if let Some(serial) = &self.serial {
            unsafe { serial.force_unlock() };
        }
    }
}

impl log::Log for LockedLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Some(serial) = &self.serial {
            let mut serial = serial.lock();
            writeln!(
                serial,
                "[zenos_bootloader: {}]: {}",
                record.level(),
                record.args()
            )
            .unwrap();
        }
    }

    fn flush(&self) {}
}
