#![allow(unused_assignments)]
mod close;
mod errors;
mod exec;
mod fork;
mod lseek;
mod open;
mod pause;
mod read;
mod table;
mod write;

use crate::{
    interrupts::gdt::GDT,
    memory::{PAGE_4K, virt_to_phys},
    process::{PROCESSES, ProcessState, current_pid},
};
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Deref;
use core::ptr;
use log::{debug, info};
use x86_64::VirtAddr;

use crate::process::FxSaveArea;
use core::arch::global_asm;

// Syscall entry that saves full register state for fork() support
// On syscall entry: RCX = user RIP, R11 = user RFLAGS, RAX = syscall number
global_asm!(
    r#"
.global syscall_entry
syscall_entry:
    swapgs
    mov gs:[0x18], rsp          // Save user RSP to scratch[0]
    mov rsp, gs:[0x10]          // Switch to kernel stack

    // Build a SyscallFrame on the stack (must match SyscallFrame struct layout)
    // Push in reverse order of struct fields
    push gs:[0x18]              // user_rsp
    push r11                    // user_rflags (saved by syscall instruction)
    push rcx                    // user_rip (saved by syscall instruction)
    push r15
    push r14
    push r13
    push r12
    push r11                    // r11 (clobbered, but save original from above)
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx                    // rcx (clobbered, but save original user_rip)
    push rbx
    push rax                    // syscall number

    // Pass pointer to SyscallFrame as first argument
    mov rdi, rsp
    
    call syscall_main

    // Restore registers from frame
    add rsp, 8                  // skip rax (return value is in rax)
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    pop rcx                     // user_rip -> rcx for sysret
    pop r11                     // user_rflags -> r11 for sysret

    cli
    swapgs
    pop rsp                     // restore user RSP
    sysretq
"#
);

/// Syscall frame saved by assembly entry point
/// Layout must match the push order in syscall_entry assembly
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub rax: u64, // syscall number
    pub rbx: u64,
    pub rcx: u64, // clobbered by syscall, contains user_rip
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64, // clobbered by syscall, contains user_rflags
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub user_rip: u64,
    pub user_rflags: u64,
    pub user_rsp: u64,
}

unsafe extern "C" {
    pub fn syscall_entry();
}

/// Convert a SyscallFrame to a ProcessState for fork support
impl SyscallFrame {
    pub fn to_process_state(&self) -> ProcessState {
        ProcessState {
            rax: self.rax,
            rbx: self.rbx,
            rcx: self.rcx,
            rdx: self.rdx,
            rsi: self.rsi,
            rdi: self.rdi,
            rbp: self.rbp,
            rsp: self.user_rsp,
            r8: self.r8,
            r9: self.r9,
            r10: self.r10,
            r11: self.r11,
            r12: self.r12,
            r13: self.r13,
            r14: self.r14,
            r15: self.r15,
            rip: self.user_rip,
            rflags: self.user_rflags,
            cs: (GDT.user_code_segment.0 | 3) as u64,
            ss: (GDT.user_data_segment.0 | 3) as u64,
            fxsave: FxSaveArea::new(),
        }
    }
}

///
/// # Safety
/// one word: Syscall.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall_main(frame: *mut SyscallFrame) -> u64 {
    let frame = unsafe { &*frame };
    let syscall_num = frame.rax;
    let rdi = frame.rdi;
    let rsi = frame.rsi;
    let rdx = frame.rdx;
    let r10 = frame.r10;
    let r8 = frame.r8;
    let r9 = frame.r9;

    let curr_pid = unsafe { (*crate::percpu::get_percpu_data()).curr_pid };

    debug!("syscall num: {} (pid={})", syscall_num, curr_pid);
    debug!(
        "args: rdi={:#x}, rsi={:#x}, rdx={:#x}, r10={:#x}, r8={:#x}, r9={:#x}",
        rdi, rsi, rdx, r10, r8, r9
    );
    info!("percpu data ptr: {:#?}", *crate::percpu::get_percpu_data());

    // Update current process state from syscall frame (needed for fork)
    {
        let pid = current_pid();
        let mut procs = PROCESSES.lock();
        if let Some(proc) = procs.iter_mut().find(|p| p.pid == pid) {
            proc.state = frame.to_process_state();
        }
    }

    let mut ret = 0;
    let table = table::SYSCALL_TABLE.deref();
    if syscall_num >= table.len() as u64 {
        info!("invalid syscall number: {}", syscall_num);
        return (-errors::ENOSYS) as u64;
    }
    let syscall = table[syscall_num as usize];
    if let Some(func) = syscall {
        ret = func(rdi, rsi, rdx, r10, r8, r9);
    } else {
        info!("invalid syscall number: {}", syscall_num);
        ret = (-errors::ENOSYS) as u64;
    }
    ret
}

#[allow(unreachable_code, unused_variables, unused_assignments)]
pub fn init() {
    let rt_ptr = syscall_entry as *const () as u64;

    let user_base = (GDT.user_data_segment.0 - 8) | 3;
    let kernel_cs = GDT.code_selector.0 as u64;

    // STAR only wants selectors
    let star_val = ((user_base as u64) << 48) | (kernel_cs << 32);
    let mut star = x86_64::registers::model_specific::Msr::new(0xC0000081);
    unsafe { star.write(star_val) }

    // LSTAR = entry point
    let mut lstar = x86_64::registers::model_specific::Msr::new(0xC0000082);
    unsafe { lstar.write(rt_ptr) }

    let mut efer = x86_64::registers::model_specific::Msr::new(0xC0000080);
    let val = unsafe { efer.read() };
    unsafe { efer.write(val | 1) }; // set SCE bit
}

/// Quickly checks whether the entire user range [ptr, ptr+len) is mapped in the
/// current page tables. This avoids taking a page fault when copying.
pub(crate) fn user_range_is_mapped(user_ptr: *const u8, len: usize) -> bool {
    if user_ptr.is_null() || len == 0 {
        return false;
    }

    let start = user_ptr as u64;
    let end = match start.checked_add(len as u64) {
        Some(v) => v,
        None => return false,
    };

    // Walk each 4 KiB page in the range and ensure it has a valid mapping.
    let mut addr = start & !(PAGE_4K as u64 - 1);
    while addr < end {
        let virt = match VirtAddr::try_new(addr) {
            Ok(v) => v,
            Err(_) => return false,
        };

        if virt_to_phys(virt).is_none() {
            return false;
        }

        addr = match addr.checked_add(PAGE_4K as u64) {
            Some(v) => v,
            None => return false,
        };
    }

    true
}

/// Copies data from a user-space pointer to a kernel-owned buffer.
/// Returns `Ok(Vec<u8>)` if successful, `Err(())` if anything looks sketchy.
///
/// Safety: This assumes the pointer and length are from user space, so we must
/// be paranoid and check for nulls, overflows, and nonsense.
fn copy_from_user(user_ptr: *const u8, len: usize) -> Result<Vec<u8>, ()> {
    //todo: support unaligned reads
    info!("copy from user {:x} len {len:x}", user_ptr as usize);

    if !user_range_is_mapped(user_ptr, len) {
        return Err(());
    }

    let mut buf = vec![0u8; len];

    // SAFETY:
    // - `user_ptr` has been validated as mapped and readable.
    // - buf has enough space for `len` bytes.
    unsafe {
        ptr::copy_nonoverlapping(user_ptr, buf.as_mut_ptr(), len);
    }

    Ok(buf)
}

/// Copies data from a kernel-owned buffer to a user-space pointer.
/// Returns `Ok(())` if successful, `Err(())` on invalid pointers or overflow.
fn copy_to_user(user_ptr: *mut u8, buf: &[u8]) -> Result<(), ()> {
    info!("copy to user {:x} len {:x}", user_ptr as usize, buf.len());

    let len = buf.len();
    if len == 0 {
        return Ok(());
    }

    if !user_range_is_mapped(user_ptr as *const u8, len) {
        return Err(());
    }

    unsafe {
        ptr::copy_nonoverlapping(buf.as_ptr(), user_ptr, len);
    }

    Ok(())
}
