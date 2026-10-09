use core::time::Duration;
use kprimitives::time::Instant;

/// Abstraction over a hardware timer for scheduler timekeeping.
///
/// The scheduler uses this trait to get the current time and program the next
/// timer interrupt. This allows the scheduler to be tested with a fake timer
/// and used with real hardware timers in production.
pub trait Timer {
    /// Returns the current time.
    fn now(&self) -> Instant;

    /// Programs the timer to fire after `duration`.
    ///
    /// The timer should fire as close to `duration` as possible. The scheduler
    /// will call this to schedule both the next quantum expiry and the next
    /// sleeper wakeup.
    fn program_next(&self, duration: Duration);
}
