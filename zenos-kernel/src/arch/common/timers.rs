use core::ops::{Add, Deref, DerefMut, Sub};
use core::time::Duration;
use kprimitives::alloc::KernelObject;

/// Represents a point in time.
///
/// This is a thin wrapper around a raw timestamp value(timestamps are timer-specific,
/// and it is a bad idea™ to use timer A's timestamp with timer B's conversion factors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct Instant(u128);

impl Instant {
    pub fn new(timestamp: u128) -> Self {
        Self(timestamp)
    }
}

impl Deref for Instant {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Add for Instant {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Instant {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl DerefMut for Instant {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(unused)]
/// Represents a clock source that can be used to measure time.
pub trait Clocksource: KernelObject {
    /// Configures the clock source, setting up any necessary hardware or software state.
    fn configure(&mut self) -> Result<(), TimerErrors>;
    /// Returns the current time as an [`Instant`].
    fn now(&self) -> Instant;
    /// Returns the duration between two [`Instant`]s.
    fn delta(&self, a: Instant, b: Instant) -> Duration;
    /// Cleans up any resources used by the clock source.
    /// The clocksource will no longer be usable after this is called.
    fn _cleanup(&mut self) -> Result<(), TimerErrors> {
        Ok(())
    }
    /// Returns the duration between an [`Instant`] and the current time
    #[inline(always)]
    fn delta_now(&self, a: Instant) -> Duration {
        self.delta(a, self.now())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerErrors {
    UnsupportedCpu,
    UnavailableDevice,
}

/// Calculate fixed-point conversion parameters.
///
/// The resulting conversion approximates:
///
///     (cycles * mult) >> shift
///
/// as:
///
///     cycles * to / from
///
/// This is conceptually similar to Linux's clocks_calc_mult_shift().
///
/// Returns `(mult, shift)`.
pub fn clocks_calc_mult_shift(to: u64, from: u64, maxsec: u64) -> (u32, u32) {
    assert!(to > 0);
    assert!(from > 0);

    let mut best_mult = 0u32;
    let mut best_shift = 0u32;

    // Linux limits the shift so that the multiplication remains
    // representable in the available integer width.
    //
    // We search from a large shift downward. A larger shift gives
    // better fixed-point precision, provided the multiplication
    // does not overflow.
    for shift in (0..=32).rev() {
        let scaled_to = (to as u128) << shift;

        let mult = scaled_to / from as u128;

        if mult == 0 || mult > u32::MAX as u128 {
            continue;
        }

        // Make sure the maximum expected cycle count can be
        // multiplied by `mult` without overflowing u64.
        //
        // maxsec * from = maximum number of source cycles.
        let max_cycles = (maxsec as u128) * (from as u128);

        if max_cycles.saturating_mul(mult) > u64::MAX as u128 {
            continue;
        }

        best_mult = mult as u32;
        best_shift = shift;

        break;
    }

    debug_assert!(best_mult != 0, "could not find a valid mult/shift pair");

    (best_mult, best_shift)
}

/// Convert source clock cycles to the destination unit.
///
/// This performs:
///
///     (cycles * mult) >> shift
///
/// using u128 internally so the intermediate multiplication
/// doesn't overflow.
pub fn cycles_to_time(cycles: u64, mult: u32, shift: u32) -> u64 {
    let result = (cycles as u128) * (mult as u128);
    (result >> shift) as u64
}
