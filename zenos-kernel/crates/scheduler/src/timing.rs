use core::time::Duration;
use kprimitives::time::Instant;

pub trait Timer {
    fn now(&self) -> Instant;
    fn program_next(&self, duration: Duration);
}
