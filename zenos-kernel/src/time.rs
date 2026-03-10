//! This module provides an interface to read the Real-Time Clock (RTC) using the CMOS registers.
//! A similar version is used in the bootloader, for logs, but this one is optimized for the kernel's needs.
//!
//! After boot, the module tracks time using a combination of the initial RTC reading
//! and the kernel tick counter (10ms per tick) for efficient timestamp generation.

use crate::arch::{inb, outb};
use core::fmt::{self, Display, Formatter};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const CMOS_ADDRESS: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;
const CURRENT_YEAR: u16 = 2026;

/// Optional CMOS register for the century (if your BIOS/ACPI supports it)
static CENTURY_REGISTER: u8 = 0x00;

/// Boot time in seconds since midnight (set once at first RTC read)
static BOOT_TIME_SECS: AtomicU64 = AtomicU64::new(0);
/// Boot date info
static BOOT_DAY: AtomicU64 = AtomicU64::new(0);
static BOOT_MONTH: AtomicU64 = AtomicU64::new(0);
static BOOT_YEAR: AtomicU64 = AtomicU64::new(0);
/// Whether boot time has been initialized
static BOOT_TIME_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Global tick counter incremented by the timer interrupt (10ms per tick)
pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Increment the tick counter (called from timer interrupt)
pub fn tick() {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get the current time using boot RTC + tick counter for efficiency
pub fn current_time() -> CurrentTime {
    if !BOOT_TIME_INITIALIZED.load(Ordering::Acquire) {
        // First call - read RTC and initialize boot time
        let rtc = CurrentTime::read_rtc();
        let boot_secs = rtc.to_seconds_since_midnight();
        BOOT_TIME_SECS.store(boot_secs, Ordering::Release);
        BOOT_DAY.store(rtc.day as u64, Ordering::Release);
        BOOT_MONTH.store(rtc.month as u64, Ordering::Release);
        BOOT_YEAR.store(rtc.year as u64, Ordering::Release);
        BOOT_TIME_INITIALIZED.store(true, Ordering::Release);
        return rtc;
    }

    // Use tick counter for subsequent calls
    let boot_secs = BOOT_TIME_SECS.load(Ordering::Acquire);
    let ticks = TICK_COUNT.load(Ordering::Relaxed);
    let elapsed_secs = ticks / 100; // 10ms per tick = 100 ticks per second

    let total_secs = boot_secs + elapsed_secs;

    // Reconstruct time with boot date
    let day = BOOT_DAY.load(Ordering::Acquire) as u8;
    let month = BOOT_MONTH.load(Ordering::Acquire) as u8;
    let year = BOOT_YEAR.load(Ordering::Acquire) as u16;

    CurrentTime::from_seconds_with_date(total_secs, day, month, year)
}

#[derive(Debug, Clone, Copy)]
/// Represents the current time read from the RTC.
/// This structure contains fields for seconds, minutes, hours, day of the month,
/// month, and full year.
///
/// The time is read from the CMOS registers, which are part of the RTC interrupts.
pub struct CurrentTime {
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

impl CurrentTime {
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

    /// Convert time to seconds since midnight
    fn to_seconds_since_midnight(&self) -> u64 {
        self.hour as u64 * 3600 + self.minute as u64 * 60 + self.second as u64
    }

    /// Create CurrentTime from seconds since midnight with given date
    fn from_seconds_with_date(mut secs: u64, day: u8, month: u8, year: u16) -> Self {
        // Handle day rollover (simplified - doesn't handle month boundaries)
        let days_elapsed = secs / 86400;
        secs %= 86400;

        let hour = (secs / 3600) as u8;
        secs %= 3600;
        let minute = (secs / 60) as u8;
        let second = (secs % 60) as u8;

        Self {
            second,
            minute,
            hour,
            day: day.saturating_add(days_elapsed as u8).min(31),
            month,
            year,
        }
    }

    /// Reads the current time directly from the RTC hardware.
    /// This is slower but more accurate for initial boot time.
    fn read_rtc() -> Self {
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

    /// Reads the current time from the RTC.
    /// Uses tick-based time after first call for efficiency.
    pub fn read() -> Self {
        current_time()
    }

    pub fn as_unix_epoch(&self) -> u64 {
        const SECS_PER_MIN: u64 = 60;
        const SECS_PER_HOUR: u64 = 3600;
        const SECS_PER_DAY: u64 = 86400;

        const DAYS_IN_MONTH: [u32; 12] = [
            31, // Jan
            28, // Feb
            31, // Mar
            30, // Apr
            31, // May
            30, // Jun
            31, // Jul
            31, // Aug
            30, // Sep
            31, // Oct
            30, // Nov
            31, // Dec
        ];

        fn is_leap(year: u32) -> bool {
            (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
        }

        let year = self.year as u32;
        let mut days: u64 = 0;

        // Years since 1970
        for y in 1970..year {
            days += if is_leap(y) { 366 } else { 365 } as u64;
        }

        // Months of current year
        for m in 0..(self.month as usize - 1) {
            days += DAYS_IN_MONTH[m] as u64;
            if m == 1 && is_leap(year) {
                days += 1;
            }
        }

        // Days in current month
        days += (self.day as u64) - 1;

        days * SECS_PER_DAY
            + (self.hour as u64) * SECS_PER_HOUR
            + (self.minute as u64) * SECS_PER_MIN
            + self.second as u64
    }
}

impl Display for CurrentTime {
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

fn local_time_to_unix_epoch() -> u64 {
    CurrentTime::read().as_unix_epoch()
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;
    use crate::test_assert_eq as assert_eq;

    #[zenos_macros::test]
    pub fn test_bcd_to_bin_zero() -> Option<()> {
        assert_eq!(bcd_to_bin(0x00), 0);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_bcd_to_bin_single_digit() -> Option<()> {
        assert_eq!(bcd_to_bin(0x05), 5);
        assert_eq!(bcd_to_bin(0x09), 9);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_bcd_to_bin_double_digit() -> Option<()> {
        assert_eq!(bcd_to_bin(0x12), 12);
        assert_eq!(bcd_to_bin(0x59), 59);
        assert_eq!(bcd_to_bin(0x99), 99);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_to_seconds_since_midnight() -> Option<()> {
        let time = CurrentTime {
            second: 30,
            minute: 15,
            hour: 10,
            day: 1,
            month: 1,
            year: 2023,
        };
        // 10*3600 + 15*60 + 30 = 36000 + 900 + 30 = 36930
        assert_eq!(time.to_seconds_since_midnight(), 36930);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_from_seconds_with_date() -> Option<()> {
        let time = CurrentTime::from_seconds_with_date(36930, 15, 6, 2025);
        assert_eq!(time.hour, 10);
        assert_eq!(time.minute, 15);
        assert_eq!(time.second, 30);
        assert_eq!(time.day, 15);
        assert_eq!(time.month, 6);
        assert_eq!(time.year, 2025);
        Some(())
    }
}
