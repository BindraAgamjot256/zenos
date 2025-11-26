//! This module provides an interface to read the Real-Time Clock (RTC) using the CMOS registers.
//! It is duplicated in the zenos-bootloader crate(logger implementation) for bootloader use.

use crate::arch::{inb, outb};
use core::fmt::{self, Display, Formatter};
const CMOS_ADDRESS: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;
const CURRENT_YEAR: u16 = 2023;

/// Optional CMOS register for the century (if your BIOS/ACPI supports it)
static CENTURY_REGISTER: u8 = 0x00;

#[derive(Debug, Clone, Copy)]
/// Represents the current time read from the RTC.
/// This structure contains fields for seconds, minutes, hours, day of the month,
/// month, and full year.
///
/// The time is read from the CMOS registers, which are part of the RTC interrupts.
pub struct RtcTime {
    /// The second (0-59)
    pub second: u8,
    /// The minute (0-59)
    pub minute: u8,
    /// The hour (0-23)
    pub hour: u8,
    /// The day of the month (1-31)
    pub day: u8,
    /// The month (1-12)
    pub month: u8,
    /// The full year (e.g., 2023)
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

    /// Checks if the RTC is currently updating.
    unsafe fn is_updating() -> bool {
        unsafe {
            outb(CMOS_ADDRESS, 0x0A);
            inb(CMOS_DATA) & 0x80 != 0
        }
    }

    /// Reads the current time from the RTC.
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
                current.6 = if CENTURY_REGISTER != 0 {
                    Self::read_cmos(CENTURY_REGISTER)
                } else {
                    0
                };

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
                if CENTURY_REGISTER != 0 {
                    cent = bcd_to_bin(cent);
                }
            }

            // Convert 12-hour to 24-hour
            if register_b & 0x02 == 0 && current.2 & 0x80 != 0 {
                hr = (hr + 12) % 24;
            }

            // Full year calculation
            let full_year = if CENTURY_REGISTER != 0 {
                (cent as u16) * 100 + yr as u16
            } else {
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
