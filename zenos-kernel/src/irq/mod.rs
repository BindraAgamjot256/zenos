use crate::{
    arch::{
        InterruptContext, InterruptGuard,
        interrupts::controller::{Clockevent, InterruptController},
        register_interrupt_handler,
    },
    mm::GlobalAllocator,
};
use kprimitives::{alloc::boxed::KBox, bitmap_allocator::BitmapAllocator, rwlock::RwLock};

pub struct IrqController {
    bitmap: BitmapAllocator<256>,
    controller: Option<KBox<dyn InterruptController + Send + Sync, GlobalAllocator>>,
}

// todo: allocate with a gsi as well...
impl IrqController {
    pub const fn new() -> Self {
        Self {
            bitmap: BitmapAllocator::new(),
            controller: None,
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

    pub fn timer(&self) -> Option<(KBox<dyn Clockevent, GlobalAllocator>, IrqGuard<'_>)> {
        let timer = self.controller.as_ref()?.timer();
        Some((timer, self.alloc_irq()?))
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

pub static IRQ_CONTROLLER: RwLock<IrqController> = RwLock::new(IrqController::new());

pub fn init(controller: KBox<dyn InterruptController + Send + Sync, GlobalAllocator>) {
    IRQ_CONTROLLER.write().set_controller(controller);

    #[cfg(target_arch = "x86_64")]
    IRQ_CONTROLLER.write().reserve_range(0, 32); // reserve first 32 IRQs as system IRQs
}
