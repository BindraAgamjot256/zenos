//! Interrupt handling support for the x86_64 architecture.
//!
//! This subsystem wires the CPU's interrupt descriptor table (IDT) to Rust
//! handlers, exposes a runtime registry for temporary or permanent overrides,
//! and provides the context structure used by exception and interrupt stubs.
//! The public API is intentionally small so higher-level kernel code can hook
//! into interrupts without reaching into the lower-level assembly details.
#![expect(unused_imports)]

mod controller;
mod ctx;
mod handlers;
mod registry;
mod table;

use crate::{
    arch::interrupts::controller::InterruptController, firmware::RuntimeBootInfo,
    mm::GlobalAllocator,
};
pub use ctx::CpuContext;
use kprimitives::{alloc::boxed::KBox, rwlock::RwLock};
pub use registry::{
    _with_handler, InterruptGuard, deregister_interrupt_handler, register_interrupt_handler,
};
use table::init_idt;

pub fn init(bootdata: &RuntimeBootInfo) {
    init_idt();
    let controller = controller::init(bootdata).expect("Controller init fails.");
    crate::irq::init(controller);
}

#[macro_export]
macro_rules! enable_interrupts {
    () => {
        unsafe {
            core::arch::asm!("sti");
        }
    };
}

#[macro_export]
macro_rules! disable_interrupts {
    () => {
        unsafe {
            core::arch::asm!("cli");
        }
    };
}

#[macro_export]
macro_rules! halt_and_catch_fire {
    () => {
        unsafe {
            core::arch::asm!("hlt");
        }
        // catch fire still todo
    };
}
