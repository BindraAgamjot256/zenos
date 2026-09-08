use super::mmio::LapicRegsMmio;
use crate::{
    arch::{
        PhysAddr,
        interrupts::controller::Clockevent,
        mem::ioremap,
        timers::{Clocksource, TimerErrors},
    },
    mm::GlobalAllocator,
};
use core::{hint::spin_loop, time::Duration};
use kprimitives::alloc::{CreatableKernelObject, KernelObject, boxed::KBox};

#[derive(Debug)]
pub struct LocalApic {
    base_addr: usize,
    ticks_per_ms: u64,
}

impl LocalApic {
    pub const fn new() -> Self {
        Self {
            base_addr: 0,
            ticks_per_ms: 0,
        }
    }

    fn regs(&self) -> *mut LapicRegsMmio {
        core::ptr::with_exposed_provenance_mut(self.base_addr)
    }

    pub fn configure<'a>(
        &mut self,
        clocksource: &KBox<dyn Clocksource + 'a + Send + Sync, GlobalAllocator>,
        lapic_base: PhysAddr,
    ) -> Option<()> {
        let virt_lapic_base = ioremap(lapic_base, size_of::<LapicRegsMmio>())
            .map_err(|e| log::error!("error: {:?}", e))
            .ok()?;
        self.base_addr = virt_lapic_base.as_usize();
        let regs = self.regs();
        unsafe { (&raw mut (*regs).timer_divide_configuration.val).write_volatile(0xB) };
        unsafe { (&raw mut (*regs).lvt_timer.val).write_volatile(1 << 16) };

        let initial_count = u32::MAX;
        unsafe { (&raw mut (*regs).timer_initial_count.val).write_volatile(initial_count) };
        let start = clocksource.now();
        while clocksource.delta_now(start) < Duration::from_millis(1) {
            spin_loop();
        }

        let remaining = unsafe { (&raw mut (*regs).timer_current_count.val).read_volatile() };

        /*
         * LAPIC counts downward:
         *
         *     initial_count
         *          |
         *          v
         *       0xFFFF
         *          |
         *     (1 ms later)
         *          |
         *          v
         *       0x1234 <- STOP HERE
         *          |
         *          v
         *          0
         *
         * Therefore:
         *
         *     elapsed = initial - remaining
         */
        let elapsed_ticks = initial_count.wrapping_sub(remaining);

        unsafe { (&raw mut (*regs).timer_initial_count.val).write_volatile(0) };

        self.ticks_per_ms = elapsed_ticks as u64;

        log::info!(
            "APIC timer configured: ticks_per_ms = {}\n({:?})",
            self.ticks_per_ms,
            self
        );
        Some(())
    }

    fn set_interrupt_vector(&self, vector: u8) {
        let regs = self.regs();
        let val = unsafe { (&raw mut (*regs).lvt_timer.val).read_volatile() };
        let new_val = (val & !0xff) | vector as u32;
        unsafe { (&raw mut (*regs).lvt_timer.val).write_volatile(new_val) };
    }

    pub fn send_eoi(&self) {
        let regs = self.regs();
        unsafe { (&raw mut (*regs).eoi.val).write_volatile(0) };
    }
}

impl KernelObject for LocalApic {}
impl CreatableKernelObject for LocalApic {
    type Allocator = crate::mm::GlobalAllocator;
}

impl Clockevent for LocalApic {
    fn next_tick(&self, then: Duration) {
        let time = then.as_millis();
        let ticks = (time * self.ticks_per_ms as u128) as u32;
        let regs = self.regs();
        unsafe { (&raw mut (*regs).timer_initial_count.val).write_volatile(ticks) };
    }

    fn toggle(&self) {
        let regs = self.regs();
        let val = unsafe { (&raw mut (*regs).lvt_timer.val).read_volatile() };
        let new_val = val ^ (1 << 16);
        unsafe { (&raw mut (*regs).lvt_timer.val).write_volatile(new_val) };
    }

    fn set_vector(&self, vector: u8) {
        self.set_interrupt_vector(vector);
    }
}
