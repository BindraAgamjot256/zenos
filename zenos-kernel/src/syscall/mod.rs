use crate::interrupts::gdt::GDT;
use core::arch::global_asm;
use log::error;

global_asm!(
    r#"
.global sys_rt0

sys_rt0:
    push rcx
    push r11

    call syscall_main

    pop r11
    pop rcx
    sysretq
    "#
);

unsafe extern "C" {
    fn sys_rt0();
}

#[unsafe(no_mangle)]
extern "C" fn syscall_main() {
    error!("Syscalls are not implemented yet.");
}

pub fn init() {
    let rt_ptr = sys_rt0 as *const () as u64;

    let user_cs = (GDT.user_code_segment.0 | 3) as u64;
    let kernel_cs = (GDT.code_selector.0 | 0) as u64;

    // STAR only wants selectors
    let star_val = (user_cs << 48) | (kernel_cs << 32);
    let mut star = x86_64::registers::model_specific::Msr::new(0xC0000081);
    unsafe { star.write(star_val) }

    // LSTAR = entry point
    let mut lstar = x86_64::registers::model_specific::Msr::new(0xC0000082);
    unsafe { lstar.write(rt_ptr) }

    // FMASK (disable interrupts during syscall)
    let mut fmask = x86_64::registers::model_specific::Msr::new(0xC0000084);
    unsafe { fmask.write(1 << 9) } // Clear IF

    let mut efer = x86_64::registers::model_specific::Msr::new(0xC0000080);
    let val = unsafe { efer.read() };
    unsafe { efer.write(val | 1) }; // set SCE bit
}
