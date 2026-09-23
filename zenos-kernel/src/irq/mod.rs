use crate::{
    arch::{
        CpuContext as InterruptContext, InterruptGuard,
        interrupts::controller::{Clockevent, InterruptController},
        register_interrupt_handler,
    },
    mm::GlobalAllocator,
};
use alloc::collections::{BTreeMap, btree_map::Entry};
use core::{
    mem,
    sync::atomic::{AtomicU8, Ordering},
};
use kprimitives::{alloc::boxed::KBox, bitmap_allocator::BitmapAllocator, rwlock::RwLock};

pub struct IrqController {
    bitmap: BitmapAllocator<256>,
    controller: Option<KBox<dyn InterruptController + Send + Sync, GlobalAllocator>>,
    locked_irqs: RwLock<BTreeMap<u8, fn(&mut InterruptContext)>>,
}

impl IrqController {
    pub const fn new() -> Self {
        Self {
            bitmap: BitmapAllocator::new(),
            controller: None,
            locked_irqs: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn set_controller(
        &mut self,
        controller: KBox<dyn InterruptController + Send + Sync, GlobalAllocator>,
    ) {
        if self.controller.is_some() {
            panic!("Controller already set");
        }
        self.controller = Some(controller);
    }

    pub fn alloc_irq<'a>(&'a self) -> Option<IrqGuard<'a>> {
        let irq = self.bitmap.alloc()?.try_into().ok()?;
        Some(IrqGuard {
            alloc: self,
            irq,
            guard_inner: None,
            number: None,
        })
    }

    pub fn free_irq(&self, irq: u8) {
        self.bitmap.free(irq as usize);
    }

    pub fn set_number(&self, number: u32) -> Option<IrqGuard<'_>> {
        let Some(controller) = &self.controller else {
            log::error!("No controller set");
            return None;
        };
        let irq = self.bitmap.alloc()?.try_into().ok()?;
        let res = controller.unmask(number, irq);
        if res.is_err() {
            log::error!("Failed to unmask IRQ {}", irq);
            return None;
        }
        Some(IrqGuard {
            alloc: self,
            irq: irq as u8,
            guard_inner: None,
            number: Some(number),
        })
    }

    pub fn reserve_range(&mut self, start: u8, end: u8) {
        unsafe {
            self.bitmap.reserve(start as usize..end as usize);
        }
    }

    pub fn timer(&self) -> Option<(KBox<dyn Clockevent, GlobalAllocator>, TimerIrq<'_>)> {
        static TIMER_IRQ: AtomicU8 = AtomicU8::new(0);

        let timer = self.controller.as_ref()?.timer();

        let mut irq = TIMER_IRQ.load(Ordering::Acquire);
        if irq == 0 {
            let new_irq = self.alloc_irq()?.vector();
            match TIMER_IRQ.compare_exchange(0, new_irq, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    // We won the race: set vector on timer
                    timer.set_vector(new_irq);
                    irq = new_irq;
                }
                Err(actual_irq) => {
                    // Another core initialized it first: keep actual_irq, release new_irq if needed
                    irq = actual_irq;
                }
            }
        }

        let timer_irq = TimerIrq {
            irq,
            guard_inner: None,
            alloc: self,
        };

        Some((timer, timer_irq))
    }

    pub fn send_eoi(&self) {
        self.controller.as_ref().unwrap().send_eoi();
    }
}

pub struct IrqGuard<'a> {
    alloc: &'a IrqController,
    irq: u8,
    guard_inner: Option<InterruptGuard>,
    number: Option<u32>,
}

impl IrqGuard<'_> {
    pub fn set_handler(&mut self, handler: fn(&mut InterruptContext)) {
        let guard_inner = register_interrupt_handler(self.irq, handler);
        self.guard_inner = Some(guard_inner);
    }
    pub fn vector(&self) -> u8 {
        self.irq
    }

    pub fn mask(&mut self) {
        let Some(num) = self.number else {
            return;
        };

        self.alloc
            .controller
            .as_ref()
            .unwrap()
            .mask(num)
            .expect("How the fuck is this failing now?");
    }

    pub fn unmask(&mut self) {
        let Some(num) = self.number else {
            return;
        };

        self.alloc
            .controller
            .as_ref()
            .unwrap()
            .unmask(num, self.irq as u32)
            .expect("How the fuck is this failing now?");
    }
}

impl<'a> Drop for IrqGuard<'a> {
    fn drop(&mut self) {
        self.alloc.free_irq(self.irq);
        if let Some(gsi) = self.number {
            self.alloc.controller.as_ref().unwrap().disable(gsi);
        }
    }
}

pub struct TimerIrq<'a> {
    irq: u8,
    guard_inner: Option<InterruptGuard>,
    alloc: &'a IrqController,
}

impl<'a> TimerIrq<'a> {
    pub fn set_handler(&mut self, handler: fn(&mut InterruptContext)) {
        let None = self.guard_inner else { return };
        let guard_inner = register_interrupt_handler(self.irq, handler);
        self.guard_inner = Some(guard_inner);
    }

    /// Sets the handler for this IRQ permanently, returning Err(handler) if one already exists.
    pub fn set_handler_locked(
        &mut self,
        handler: fn(&mut InterruptContext),
    ) -> Result<(), fn(&mut InterruptContext)> {
        let mut irq = self.alloc.locked_irqs.write();
        match irq.entry(self.irq) {
            Entry::Occupied(_) => Err(handler),
            Entry::Vacant(e) => {
                let guard = register_interrupt_handler(self.irq, handler);
                e.insert_entry(handler);
                mem::forget(guard);
                Ok(())
            }
        }
    }

    pub fn vector(&self) -> u8 {
        self.irq
    }
}

pub static IRQ_CONTROLLER: RwLock<IrqController> = RwLock::new(IrqController::new());

pub fn init(controller: KBox<dyn InterruptController + Send + Sync, GlobalAllocator>) {
    IRQ_CONTROLLER.write().set_controller(controller);

    #[cfg(target_arch = "x86_64")]
    IRQ_CONTROLLER.write().reserve_range(0, 32); // reserve first 32 IRQs as system IRQs
}
