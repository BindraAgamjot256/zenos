//! Interrupt handling support for the x86_64 architecture.
//!
//! This subsystem wires the CPU's interrupt descriptor table (IDT) to Rust
//! handlers, exposes a runtime registry for temporary or permanent overrides,
//! and provides the context structure used by exception and interrupt stubs.
//! The public API is intentionally small so higher-level kernel code can hook
//! into interrupts without reaching into the lower-level assembly details.

mod ctx;
mod table;
mod registry;
mod handlers;

pub use table::init_idt;

#[allow(dead_code, unused_imports)]
pub use registry::{deregister_interrupt_handler, register_interrupt_handler, with_handler};