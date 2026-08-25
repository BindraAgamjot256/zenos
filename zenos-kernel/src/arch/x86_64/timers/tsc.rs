use crate::arch::timers::*;
use core::arch::x86_64::{__cpuid, _rdtsc};
use core::time::Duration;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

/// A TSC-based timer that uses CPUID to determine the crystal frequency and calculates
/// the multiplier and shift values for converting TSC ticks to nanoseconds.
///
/// Can be used as a [`Clocksource`] on x86_64.
///
/// [`Instant`]s for this timer are nanosecond-based.
pub struct TscTimer {
    mult: u32,
    shift: u32,
}

impl TscTimer {
    pub const fn new() -> Self {
        Self { mult: 0, shift: 0 }
    }

    fn configure_cpuid(&mut self) -> Result<(), TimerErrors> {
        let cpuid = __cpuid(0x80000007);

        if cpuid.edx & (1 << 8) == 0 {
            return Err(TimerErrors::UnavailableDevice);
        }

        let cpuid = __cpuid(0x15);

        let den = cpuid.eax as u64;
        let num = cpuid.ebx as u64;
        let crystal = cpuid.ecx as u64;

        if den == 0 || num == 0 || crystal == 0 {
            return Err(TimerErrors::UnavailableDevice);
        }

        let freq = num * crystal / den;

        let (mult, shift) = clocks_calc_mult_shift(1_000_000_000, freq, 600);

        self.mult = mult;
        self.shift = shift;

        Ok(())
    }

    fn read(&self) -> Instant {
        let ticks = unsafe { _rdtsc() };
        Instant::new(cycles_to_time(ticks, self.mult, self.shift) as u128)
    }
}

impl KernelObject for TscTimer {}
impl CreatableKernelObject for TscTimer {
    type Allocator = crate::mm::GlobalAllocator;
}

impl Clocksource for TscTimer {
    fn configure(&mut self) -> Result<(), TimerErrors> {
        let max_extended = __cpuid(0x80000000);
        let max_basic = __cpuid(0);

        if max_extended.eax < 0x80000007 || max_basic.eax < 0x15 {
            // todo: configure using the msrs instead.
            return Err(TimerErrors::UnsupportedCpu);
        }

        self.configure_cpuid()
    }

    fn now(&self) -> Instant {
        self.read()
    }

    fn delta(&self, a: Instant, b: Instant) -> Duration {
        Duration::from_nanos_u128(b.wrapping_sub(*a))
    }
}
