//! Interrupt handling support for the x86_64 architecture.
//!
//! This subsystem wires the CPU's interrupt descriptor table (IDT) to Rust
//! handlers, exposes a runtime registry for temporary or permanent overrides,
//! and provides the context structure used by exception and interrupt stubs.
//! The public API is intentionally small so higher-level kernel code can hook
//! into interrupts without reaching into the lower-level assembly details.
#![allow(unused_imports)]

mod controller;
mod ctx;
mod handlers;
mod registry;
mod table;

use crate::{
    arch::interrupts::controller::InterruptController, firmware::RuntimeBootInfo,
    mm::GlobalAllocator,
};
pub use ctx::InterruptContext;
use kprimitives::{alloc::boxed::KBox, rwlock::RwLock};
pub use registry::{deregister_interrupt_handler, register_interrupt_handler, with_handler};
use table::init_idt;

pub fn init(bootdata: &RuntimeBootInfo) {
    init_idt();
    controller::init(bootdata);
}

pub static INTERRUPT_CONTROLLER: RwLock<
    Option<KBox<dyn InterruptController + Send + Sync, GlobalAllocator>>,
> = RwLock::new(None);
