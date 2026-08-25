use crate::{
    arch::{PhysAddr, mem::ioremap, timers::Clocksource},
    mm::GlobalAllocator,
};
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

const NSEC_PER_SEC: u64 = 1_000_000_000;
const FSEC_PER_SEC: u64 = 1_000_000_000_000_000;

// Ten years of conversion range.
const MAX_SEC: u64 = 10 * 365 * 24 * 60 * 60;

#[repr(C)]
pub struct HpetRegisters {
    pub capabilities: u64, // Offset 0x00
    _reserved1: u64,       // Offset 0x08
    pub cfg: u64,          // Offset 0x10
    _reserved2: u64,       // Offset 0x18
    pub int_status: u64,   // Offset 0x20
    _reserved3: [u8; 200], // Offset 0x28 to 0xF0
    pub counter: u64,      // Offset 0xF0
}

pub struct HpetTimer {
    mult: u32,
    shift: u32,

    addr: usize,
    regs: usize,

    bits_64: bool,
}

impl HpetTimer {
    pub const fn new(addr: usize, bits_64: bool) -> Self {
        Self {
            mult: 0,
            shift: 0,
            addr,
            regs: 0,
            bits_64,
        }
    }

    #[inline]
    fn regs(&self) -> *mut HpetRegisters {
        self.regs as *mut HpetRegisters
    }

    #[inline]
    fn read_counter(&self) -> u64 {
        let regs = self.regs();

        let counter = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*regs).counter)) };

        if self.bits_64 {
            counter
        } else {
            counter as u32 as u64
        }
    }
}

impl KernelObject for HpetTimer {}

impl CreatableKernelObject for HpetTimer {
    type Allocator = GlobalAllocator;
}

impl Clocksource for HpetTimer {
    fn configure(&mut self) -> Result<(), crate::arch::timers::TimerErrors> {
        let addr = PhysAddr::new(self.addr as u64);

        let va = ioremap(addr, core::mem::size_of::<HpetRegisters>()).map_err(|_| todo!())?;

        self.regs = va.as_usize();

        let regs = self.regs();

        let capabilities =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*regs).capabilities)) };

        /*
         * HPET capabilities:
         *
         * bits 32..63:
         *     Main counter clock period in femtoseconds.
         */
        let period_fs = capabilities >> 32;

        if period_fs == 0 {
            return Err(todo!());
        }

        /*
         * Convert:
         *
         *     femtoseconds/tick
         *
         * into:
         *
         *     ticks/second
         */
        let frequency = FSEC_PER_SEC / period_fs;

        if frequency == 0 {
            return Err(todo!());
        }

        /*
         * We want:
         *
         *     cycles * mult >> shift
         *
         * to produce nanoseconds.
         *
         * Therefore:
         *
         *     to   = 1_000_000_000 ns
         *     from = HPET frequency
         */
        let (mult, shift) =
            crate::arch::common::timers::clocks_calc_mult_shift(NSEC_PER_SEC, frequency, MAX_SEC);

        self.mult = mult;
        self.shift = shift;

        /*
         * Enable HPET main counter.
         *
         * General Configuration bit 0:
         *
         *     ENABLE_CNF
         */
        let mut cfg = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*regs).cfg)) };

        cfg |= 1;

        unsafe {
            core::ptr::write_volatile(core::ptr::addr_of_mut!((*regs).cfg), cfg);
        }

        Ok(())
    }

    fn now(&self) -> crate::arch::timers::Instant {
        let counter = self.read_counter();

        let ns = crate::arch::common::timers::cycles_to_time(counter, self.mult, self.shift);

        crate::arch::timers::Instant::new(ns as u128)
    }

    fn delta(
        &self,
        a: crate::arch::timers::Instant,
        b: crate::arch::timers::Instant,
    ) -> core::time::Duration {
        let a_ns = *a;
        let b_ns = *b;

        core::time::Duration::from_nanos_u128(b_ns.wrapping_sub(a_ns))
    }
}
