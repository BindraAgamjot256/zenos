mod write;

use crate::interrupts::gdt::GDT;
use crate::kprint;
use crate::syscall::write::FileDescriptor;
use core::arch::{asm, global_asm};
use core::slice;
use log::debug;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::idt::InterruptStackFrame;

#[unsafe(no_mangle)]
pub extern "x86-interrupt" fn sys_rt0(interupt_stack_frame: InterruptStackFrame) {
    let mut syscall_num: u64;
    let mut user_rip: u64;
    let mut rflags: u64;
    let mut rdi_val: u64;
    let mut rsi_val: u64;
    let mut rdx_val: u64;
    let mut r10_val: u64;
    let mut r8_val: u64;
    let mut r9_val: u64;
    let mut ret: u64 = 0;

    // Grab everything up front, like a responsible adult.
    unsafe {
        asm!(
        "mov {syscall}, rax",
        "mov {rip}, rcx",
        "mov {rfl}, r11",
        "mov {rdi_val}, rdi",
        "mov {rsi_val}, rsi",
        "mov {rdx_val}, rdx",
        "mov {r10_val}, r10",
        "mov {r8_val}, r8",
        "mov {r9_val}, r9",
        syscall = out(reg) syscall_num,
        rip = out(reg) user_rip,
        rfl = out(reg) rflags,
        rdi_val = out(reg) rdi_val,
        rsi_val = out(reg) rsi_val,
        rdx_val = out(reg) rdx_val,
        r10_val = out(reg) r10_val,
        r8_val = out(reg) r8_val,
        r9_val = out(reg) r9_val,
        options(nostack, preserves_flags),
        );
    }
    unsafe {
        ret = syscall_main(
            syscall_num,
            rdi_val,
            rsi_val,
            rdx_val,
            r10_val,
            r8_val,
            r9_val,
        );
    }
    unsafe {
        asm!(
        "mov {ret}, rax",
        "mov {rip}, rcx",
        "mov {rfl}, r11",
        ret = out(reg) ret,
        rip = out(reg) user_rip, // just in case i need it later
        rfl = out(reg) rflags,   // see above
        options(nostack, preserves_flags),
        );
    }
}

pub unsafe fn syscall_main(
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

    match syscall_num {
        1 => {
            // write(fd, buf, len)
            let fd = rdi;
            let buf_ptr = rsi;
            let len = rdx;

            let buf = unsafe { slice::from_raw_parts(buf_ptr as *const u8, len as usize) };
            let fd = FileDescriptor::try_from(fd);
            if fd.is_err() {
                ret = u64::MAX;
            } else {
                let val = write::sys_write(buf, fd.unwrap());
                if val.is_some() {
                    ret = val.unwrap()
                } else {
                    ret = u64::MAX;
                }
            }
        }
        _ => {
            panic!("unsupported syscall number: {}", syscall_num);
        }
    }

    debug!("returning {:#x} from syscall_main", ret);

    ret
}

#[allow(unreachable_code)]
pub fn init() {
    return;
    //todo use syscall/sysret instead of int 0x80/iret
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
