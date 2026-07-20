//! Runtime registry for interrupt and exception handlers.
//!
//! The registry is the bridge between the assembly interrupt stubs and the
//! Rust callbacks that process them. It stores the currently installed handler
//! for each vector, provides a default fallback for unconfigured entries, and
//! offers a small RAII guard for temporary overrides during debugging or
//! bring-up work.

use crate::arch::x86_64::interrupts::ctx::InterruptContext;
use core::sync::atomic::Ordering;

pub(super) static IDT_REGISTRY: InterruptRegistry = InterruptRegistry::new();

/// Stores the currently registered Rust handler for each interrupt vector.
///
/// The registry is intentionally array-based so the dispatch path can resolve
/// a handler in constant time for any vector in the 0-255 range.
pub(super) struct InterruptRegistry {
    handlers: [core::sync::atomic::AtomicPtr<fn(&mut InterruptContext)>; 256],
}

fn default_handler(context: &mut InterruptContext) {
    log::error!(
        "Unhandled interrupt vector {:#x} reached the default handler. Context: {:?}",
        context.vector(),
        context
    );
    panic!(
        "Default interrupt handler called for vector {:#x}. No handler registered.",
        context.vector()
    );
}

impl InterruptRegistry {
    /// Creates an interrupt registry populated with the default panic handler.
    ///
    /// Every vector starts with a fallback handler so an unconfigured entry
    /// fails loudly instead of silently continuing with an invalid dispatch.
    pub const fn new() -> Self {
        const INIT: core::sync::atomic::AtomicPtr<fn(&mut InterruptContext)> =
            core::sync::atomic::AtomicPtr::new(default_handler as *mut fn(&mut InterruptContext));
        Self {
            handlers: [INIT; 256],
        }
    }

    /// Registers a handler for a specific interrupt vector.
    ///
    /// Replacing an existing entry updates the registry immediately for future
    /// dispatches. The previous handler is overwritten and a log entry is emitted
    /// so debugging is easier when vectors are reassigned.
    pub fn register_handler(&self, interrupt_number: u8, handler: fn(&mut InterruptContext)) {
        let index = interrupt_number as usize;
        let previous = self.handlers[index].load(Ordering::Acquire);
        if previous.is_null() {
            log::info!("Registered interrupt handler for vector {interrupt_number:#x}");
        } else {
            log::warn!("Replaced interrupt handler for vector {interrupt_number:#x}");
        }

        self.handlers[index].store(handler as *mut fn(&mut InterruptContext), Ordering::Release);
    }

    /// Returns the handler currently associated with the requested interrupt vector.
    pub fn get_handler(&self, interrupt_number: u8) -> fn(&mut InterruptContext) {
        let handler_ptr = self.handlers[interrupt_number as usize].load(Ordering::Acquire);
        if handler_ptr.is_null() {
            log::warn!(
                "No handler registered for vector {interrupt_number:#x}; using the default panic handler"
            );
            default_handler
        } else {
            unsafe { core::mem::transmute(handler_ptr) }
        }
    }
}

/// Registers a handler and returns a guard that restores the default handler on drop.
///
/// This is useful for temporary debugging hooks that should be cleaned up once
/// the surrounding operation completes.
pub fn register_interrupt_handler(
    interrupt_number: u8,
    handler: fn(&mut InterruptContext),
) -> InterruptGuard {
    IDT_REGISTRY.register_handler(interrupt_number, handler);
    InterruptGuard(interrupt_number)
}

/// Clears a handler by restoring the default panic handler for the given vector.
pub fn deregister_interrupt_handler(interrupt_number: u8) {
    log::info!("Deregistering interrupt handler for vector {interrupt_number:#x}");
    IDT_REGISTRY.register_handler(interrupt_number, default_handler);
}

/// Registers a handler without returning a guard object.
pub(super) fn register_guardless(interrupt_number: u8, handler: fn(&mut InterruptContext)) {
    IDT_REGISTRY.register_handler(interrupt_number, handler);
}

/// A small RAII guard that restores the default handler when it goes out of scope.
pub struct InterruptGuard(u8);

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        deregister_interrupt_handler(self.0);
    }
}

/// Registers a temporary handler for the duration of the provided closure.
///
/// The handler is installed before the closure runs and restored immediately
/// afterward, even if the closure panics.
pub fn with_handler(interrupt_number: u8, f: fn(&mut InterruptContext), g: impl FnOnce()) {
    let _guard = register_interrupt_handler(interrupt_number, f);
    g();
}
