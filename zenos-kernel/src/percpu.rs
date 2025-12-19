//! Per-CPU data structures and SWAPGS support for x86_64.
//!
//! This version uses the `x86_64` crate for MSR access
//! and provides safe (well, as safe as kernel code gets) abstractions.

use core::{arch::asm, marker::PhantomData, ptr};
use x86_64::registers::model_specific::Msr;

/// MSR identifiers (same as Intel SDM)
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;
const IA32_GS_BASE: u32 = 0xC000_0101;

/// Represents per-CPU data accessible via the GS segment register.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PerCpuData {
    /// ID of this CPU
    pub cpu_id: u32,
    /// Pointer to the base of this per-CPU area (self-reference)
    pub self_ptr: *mut PerCpuData,
    /// Kernel stack pointer for this CPU
    pub kernel_stack_ptr: u64,
    /// Scratch space for use during interrupts
    pub scratch: [u64; 4],

    pub curr_pid: u64,
}

impl PerCpuData {
    pub const fn new(cpu_id: u32) -> Self {
        Self {
            cpu_id,
            self_ptr: ptr::null_mut(),
            kernel_stack_ptr: 0,
            scratch: [0; 4],
            curr_pid: 0,
        }
    }
}

/// Execute SWAPGS to toggle between kernel and user GS base.
///
/// # Safety
///
/// Only call during proper privilege transitions (user↔kernel).
#[inline(always)]
pub unsafe fn swapgs() {
    asm!("swapgs", options(nostack, preserves_flags));
}

/// Initialize per-CPU data for this CPU.
///
/// # Safety
///
/// Must be called *once per CPU* before interrupts or tasks run.
/// Must be done in ring 0.
pub unsafe fn init_percpu(percpu_data: *mut PerCpuData) {
    // Set self pointer for convenience
    let data = percpu_data.as_mut_unchecked();
    data.self_ptr = percpu_data;
    let sp: u64;
    unsafe { asm!("mov {}, rsp", out(reg) sp) };
    data.kernel_stack_ptr = sp;
    data.cpu_id = 0; //todo: get from cpuid

    // Set KERNEL_GS_BASE → our per-CPU struct
    let mut kernel_gs_base = Msr::new(IA32_KERNEL_GS_BASE);
    kernel_gs_base.write(percpu_data as u64);
    // Set GS_BASE to 0 (user context)
    let mut gs_base = Msr::new(IA32_GS_BASE);
    gs_base.write(0);
    // make sure we're in kernel mode by default.
    swapgs();
}

/// Get pointer to current CPU’s `PerCpuData`.
///
/// # Safety
///
/// Must be called after `SWAPGS` or kernel entry.
#[inline(always)]
pub unsafe fn get_percpu_data() -> *mut PerCpuData {
    let ptr: *mut PerCpuData;
    asm!(
    "mov {}, gs:[0x8]", // self_ptr at offset 0x8
    out(reg) ptr,
    options(nostack, preserves_flags, readonly)
    );
    ptr
}

/// A type-safe per-CPU variable wrapper.
///
/// This lets you define per-CPU data fields with compile-time-known offsets
/// into `PerCpuData`. Each CPU accesses its own copy transparently.
pub struct PerCpuVar<T> {
    offset: usize,
    _phantom: PhantomData<T>,
}

unsafe impl<T> Sync for PerCpuVar<T> {}

impl<T> PerCpuVar<T> {
    /// Create a per-CPU variable located at a specific field offset in `PerCpuData`.
    ///
    /// # Example
    /// ```
    /// const EXAMPLE_COUNTER: PerCpuVar<u64> =
    ///     PerCpuVar::new(offset_of!(PerCpuData, example_counter));
    /// ```
    pub const fn new(offset: usize) -> Self {
        Self {
            offset,
            _phantom: PhantomData,
        }
    }
}
//todo: read/write for others also
impl PerCpuVar<u64> {
    #[inline(always)]
    pub unsafe fn read(&self) -> u64 {
        let value: u64;
        asm!(
        "mov {}, gs:[{}]",
        out(reg) value,
        in(reg) self.offset,
        options(nostack, preserves_flags, readonly)
        );
        value
    }

    #[inline(always)]
    pub unsafe fn write(&self, value: u64) {
        asm!(
        "mov gs:[{}], {}",
        in(reg) self.offset,
        in(reg) value,
        options(nostack, preserves_flags)

        );
    }
}

#[unsafe(link_section = ".percpu")]
// SAFTEY: this is safe because each static can only be accessed by one CPU at a time,
pub(crate) static mut PER_CPU_AREAS: [PerCpuData; MAX_CPUS] = [
    PerCpuData::new(0),
    PerCpuData::new(1),
    PerCpuData::new(2),
    PerCpuData::new(3),
];

const MAX_CPUS: usize = 4; //todo: get from cpuid

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::test_assert_eq as assert_eq;
    use crate::Test;
    #[zenos_macros::test]
    pub fn test_layout() -> Option<()> {
        let data = PerCpuData::new(42);
        let base = &data as *const _ as usize;
        assert_eq!((&data.cpu_id as *const _ as usize) - base, 0);
        assert_eq!((&data.self_ptr as *const _ as usize) - base, 8);
        Some(())
    }
}
