//! CPU interrupt context and interrupt stub assembly.
//!
//! The interrupt entry stubs push the register state onto the stack so Rust
//! handlers can inspect the faulting vector and instruction pointer.

pub use self::CpuContext as InterruptContext;

use core::arch::global_asm;

/// The register state captured when an interrupt or exception enters the kernel.
///
/// The layout mirrors the assembly stub that saves the CPU registers before the
/// Rust dispatcher hands control to a handler.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct CpuContext {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,

    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,

    vector_number: u64,
    error_code: u64,

    rip: u64,
    cs: u64,
    rflags: u64,

    rsp: u64,
    ss: u64,
}

impl CpuContext {
    /// Returns the error code attached to the interrupt, if one was present.
    pub fn err_code(&self) -> u64 {
        self.error_code
    }

    /// Returns the interrupt or exception vector number.
    pub fn vector(&self) -> u64 {
        self.vector_number
    }

    /// Returns the instruction pointer that triggered the exception.
    pub fn instruction_pointer(&self) -> u64 {
        self.rip
    }
}

global_asm!(
    r#"
.section .text
.altmacro

# Each interrupt stub begins by pushing a synthetic error code for vectors that
# do not deliver one automatically, then pushes the vector number so the common
# entry can reconstruct the full interrupt context.
.macro ISR_VAL vector, has_error
    .global isr_\vector
    .type isr_\vector, @function
    isr_\vector:
        .if \has_error == 0
            push 0
        .else
            nop
            nop
        .endif
        push \vector
        jmp common_interrupt_entry
.endm

# Exceptions that already push an error code on the stack are handled by the
# list below; all other vectors get the placeholder zero inserted above.
.macro EVAL_ISR current
    .if \current == 8 || \current == 10 || \current == 11 || \current == 12 || \current == 13 || \current == 14 || \current == 17 || \current == 21 || \current == 29 || \current == 30
        ISR_VAL %\current, 1
    .else
        ISR_VAL %\current, 0
    .endif
.endm

# The assembly generator expands the stubs in blocks of 16 so the table can be
# built for the full 0-255 interrupt space without handwritten repetition.
.macro generate_16_isrs start
    EVAL_ISR %(\start + 0)
    EVAL_ISR %(\start + 1)
    EVAL_ISR %(\start + 2)
    EVAL_ISR %(\start + 3)
    EVAL_ISR %(\start + 4)
    EVAL_ISR %(\start + 5)
    EVAL_ISR %(\start + 6)
    EVAL_ISR %(\start + 7)
    EVAL_ISR %(\start + 8)
    EVAL_ISR %(\start + 9)
    EVAL_ISR %(\start + 10)
    EVAL_ISR %(\start + 11)
    EVAL_ISR %(\start + 12)
    EVAL_ISR %(\start + 13)
    EVAL_ISR %(\start + 14)
    EVAL_ISR %(\start + 15)
.endm

# The common entry point preserves the interrupted register state and passes a
# pointer to the layout expected by the Rust dispatcher.
common_interrupt_entry:
    # Save the general-purpose registers so they can be inspected later.
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    # Pass the saved frame pointer to the Rust-side dispatcher.
    mov rdi, rsp
    cld

    # Reserve a slot for the Rust call so the stack layout is stable.
    sub rsp, 8
    call rust_dispatch
    add rsp, 8

    # Restore the saved state in reverse order.
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    # Drop the synthetic vector/error-slot pair and return to the interrupted code.
    add rsp, 16
    iretq

# Expand the macro into the full 256-entry ISR set.
generate_16_isrs 0
generate_16_isrs 16
generate_16_isrs 32
generate_16_isrs 48
generate_16_isrs 64
generate_16_isrs 80
generate_16_isrs 96
generate_16_isrs 112
generate_16_isrs 128
generate_16_isrs 144
generate_16_isrs 160
generate_16_isrs 176
generate_16_isrs 192
generate_16_isrs 208
generate_16_isrs 224
generate_16_isrs 240
"#
);

/// Dispatches a captured interrupt context to the currently registered Rust handler.
#[unsafe(no_mangle)]
unsafe extern "C" fn rust_dispatch(_context: *mut CpuContext) {
    let registry = &crate::arch::x86_64::interrupts::registry::IDT_REGISTRY;
    let context = unsafe { &mut *_context };
    let handler = registry.get_handler(context.vector_number as u8);
    handler(context);
}
