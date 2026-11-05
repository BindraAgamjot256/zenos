use crate::ffi;

/// The register state captured when an interrupt or exception enters the kernel.
///
/// The layout mirrors the assembly stub that saves the CPU registers before the
/// Rust dispatcher hands control to a handler.
#[derive(Debug, Clone, Default)]
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

    pub fn new_from_fn_kernel(func: fn(), stack_ptr: *mut u8) -> Self {
        Self {
            #[expect(clippy::fn_to_numeric_cast)]
            rip: func as u64,
            rflags: 1 << 9 | 1 << 1,
            cs: ffi::kernel_get_cs() as u64,
            rsp: stack_ptr.addr().try_into().unwrap(),
            ss: ffi::kernel_get_ds() as u64,
            ..Default::default()
        }
    }
}
