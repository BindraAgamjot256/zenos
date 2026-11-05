use crate::sync::spinlock::InterruptDisabler;

pub mod interrupts;
pub mod mem_types;
pub mod timers;

pub type SpinLock<T> = crate::sync::spinlock::SpinLock<T, InterruptDisabler>;
