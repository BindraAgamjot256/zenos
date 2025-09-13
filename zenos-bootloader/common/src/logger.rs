use crate::serial::SerialPort;
use bootloader_api::info::FrameBufferInfo;
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
    pub fn new(
        _framebuffer: &'static mut [u8],
        _info: FrameBufferInfo,
        _frame_buffer_logger_status: bool,
        serial_logger_status: bool,
    ) -> Self {
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
                "[{}: zenos_bootloader: {}]: {}",
                RtcTime::read(),
                record.level(),
                record.args()
            )
            .unwrap();
        }
    }

    fn flush(&self) {}
}

use core::fmt::{self, Display, Formatter};

const CMOS_ADDRESS: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;
const CURRENT_YEAR: u16 = 2023;

#[derive(Debug, Clone, Copy)]
pub struct RtcTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

impl RtcTime {
    /// Reads an RTC register via CMOS
    unsafe fn read_cmos(reg: u8) -> u8 {
        unsafe {
            outb(CMOS_ADDRESS, reg);
            inb(CMOS_DATA)
        }
    }

    unsafe fn is_updating() -> bool {
        unsafe {
            outb(CMOS_ADDRESS, 0x0A);
            inb(CMOS_DATA) & 0x80 != 0
        }
    }

    pub fn read() -> Self {
        unsafe {
            let mut last: (u8, u8, u8, u8, u8, u8, u8) = (0, 0, 0, 0, 0, 0, 0);
            let mut current: (u8, u8, u8, u8, u8, u8, u8) = (0, 0, 0, 0, 0, 0, 0);

            loop {
                while Self::is_updating() {
                    core::hint::spin_loop();
                }

                current.0 = Self::read_cmos(0x00); // second
                current.1 = Self::read_cmos(0x02); // minute
                current.2 = Self::read_cmos(0x04); // hour
                current.3 = Self::read_cmos(0x07); // day
                current.4 = Self::read_cmos(0x08); // month
                current.5 = Self::read_cmos(0x09); // year
                current.6 = 0; // todo: century register,

                if current == last {
                    break;
                }

                last = current;
            }

            let register_b = Self::read_cmos(0x0B);

            // BCD to binary
            let mut sec = current.0;
            let mut min = current.1;
            let mut hr = current.2;
            let mut day = current.3;
            let mut mon = current.4;
            let mut yr = current.5;
            let mut cent = current.6;

            if register_b & 0x04 == 0 {
                sec = bcd_to_bin(sec);
                min = bcd_to_bin(min);
                hr = bcd_to_bin(hr & 0x7F);
                day = bcd_to_bin(day);
                mon = bcd_to_bin(mon);
                yr = bcd_to_bin(yr);
                cent = bcd_to_bin(cent);
            }

            // Convert 12-hour to 24-hour
            if register_b & 0x02 == 0 && current.2 & 0x80 != 0 {
                hr = (hr + 12) % 24;
            }

            // Full year calculation
            let full_year = {
                let mut full = (CURRENT_YEAR / 100) * 100 + yr as u16;
                if full < CURRENT_YEAR {
                    full += 100;
                }
                full
            };

            Self {
                second: sec,
                minute: min,
                hour: hr,
                day,
                month: mon,
                year: full_year,
            }
        }
    }
}
impl Display for RtcTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Convert BCD to binary
const fn bcd_to_bin(val: u8) -> u8 {
    (val & 0x0F) + ((val >> 4) * 10)
}

/// Write to an I/O port
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

/// Read from an I/O port
unsafe fn inb(port: u16) -> u8 {
    unsafe {
        let value: u8;
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
        value
    }
}
