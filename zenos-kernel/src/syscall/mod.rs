#![allow(unused_assignments)]
mod close;
mod errors;
mod lseek;
mod open;
mod read;
mod table;
mod write;

use crate::{
    interrupts::gdt::GDT,
    memory::{PAGE_4K, virt_to_phys},
};
use alloc::vec;
use alloc::vec::Vec;
use core::ops::Deref;
use core::ptr;
use log::{debug, info};
use x86_64::VirtAddr;

use core::arch::global_asm;

global_asm!(
    r#"
.global syscall_entry
syscall_entry:
    swapgs
    mov gs:[0x18], rsp
    mov rsp, gs:[0x10]

    push gs:[0x18]
    push r11
    push rcx
    push r9
    push r8
    push r10
    push rdx
    push rsi
    push rdi

    mov rdi, rax
    mov rsi, [rsp]
    mov rdx, [rsp+8]
    mov rcx, [rsp+16]
    mov r8,  [rsp+24]
    mov r9,  [rsp+32]
    push [rsp+40]

    call syscall_main

    add rsp, 8

    pop rdi
    pop rsi
    pop rdx
    pop r10
    pop r8
    pop r9
    pop rcx
    pop r11

    cli
    swapgs
    pop rsp
    sysretq
"#
);

unsafe extern "C" {
    pub fn syscall_entry();
}

///
/// # Safety
/// one word: Syscall.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn syscall_main(
    syscall_num: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    r10: u64,
    r8: u64,
    r9: u64,
) -> u64 {
    let mut ret = 0;

    debug!("syscall num: {}", syscall_num);
    debug!(
        "args: rdi={:#x}, rsi={:#x}, rdx={:#x}, r10={:#x}, r8={:#x}, r9={:#x}",
        rdi, rsi, rdx, r10, r8, r9
    );
    let syscall = table::SYSCALL_TABLE.deref()[syscall_num as usize];
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
        if virt_to_phys(VirtAddr::new(addr)).is_none() {
            return false;
        }
        addr += PAGE_4K as u64;
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
