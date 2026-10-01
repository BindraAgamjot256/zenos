pub mod interrupts;
pub mod mem_types;
pub mod timers;

#[cfg(feature = "disable_int_on_lock")]
pub struct Disabler;

#[cfg(feature = "disable_int_on_lock")]
impl kprimitives::Disabler for Disabler {
    fn disable_int() -> usize {
        crate::arch::disable_interrupts_and_save()
    }

    fn enable_int(flags: usize) {
        crate::arch::restore_interrupts(flags)
    }
}

#[cfg(feature = "disable_int_on_lock")]
pub type SpinLock<T> = kprimitives::spinlock::SpinLock<T, Disabler>;

#[cfg(not(feature = "disable_int_on_lock"))]
pub type SpinLock<T> = kprimitives::spinlock::SpinLock<T>;
