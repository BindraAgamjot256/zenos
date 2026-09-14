use core::ops::{Add, Deref, DerefMut, Sub};
use core::time::Duration;

/// Represents a point in time.
///
/// Timestamps are measured in nanoseconds, and are timer-independent.
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

impl Add<Duration> for Instant {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + rhs.as_nanos())
    }
}

impl Sub<Duration> for Instant {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        Self(self.0 - rhs.as_nanos())
    }
}

impl DerefMut for Instant {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
