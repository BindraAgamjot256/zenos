//! Exception handlers used by the interrupt dispatch path.
//!
//! The current implementation focuses on the page-fault path, which is one of
//! the most informative early-stage kernel faults. The handler logs the state
//! captured by the interrupt stub and then panics so the failure is visible
//! immediately during bring-up and debugging.

// todo: custom error code types, more handlers...

/// Handles page faults by reporting the faulting address and panicking.
///
/// The handler reads the faulting address from CR2, records the interrupt
/// vector, error code, and instruction pointer, and then aborts so the kernel
/// does not continue from an invalid memory access.
pub fn pf_handler(context: &mut super::ctx::CpuContext) {
    let cr2 = crate::arch::x86_64::registers::control::CR2::read()
        .map(|v| v.as_usize())
        .unwrap_or(0xdeadbeef);

    log::error!(
        "Page fault at address {:#x} while handling vector {:#x} (err code: {:?}, rip: {:#x})",
        cr2,
        context.vector(),
        context.err_code(),
        context.instruction_pointer()
    );

    panic!("Page fault occurred. Address: {:#x}", cr2);
}

pub fn df_handler(context: &mut super::ctx::CpuContext) {
    panic!("DOUBLE FAULT, CTX: {context:#?}")
}
