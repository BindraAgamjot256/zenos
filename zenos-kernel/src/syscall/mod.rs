use crate::interrupts::gdt::GDT;
use crate::kprint;
use core::arch::global_asm;
use core::slice;
use log::{debug, error};

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
    use core::arch::asm;

    let mut syscall_num: u64;
    let mut fd: u64;
    let mut buf_ptr: u64;
    let mut len: u64;
    let mut ret: u64 = 0;

    unsafe {
        asm!(
        "mov {}, rax",
        out(reg) syscall_num
        );
        match syscall_num {
            1 => {
                // write(fd, buf, len)
                asm!("mov {}, rdi", out(reg) fd);
                asm!("mov {}, rsi", out(reg) buf_ptr);
                asm!("mov {}, rdx", out(reg) len);
                if fd == 1 || fd == 2 {
                    let buf = slice::from_raw_parts(buf_ptr as *const u8, len as usize);
                    kprint!("{}", core::str::from_utf8_unchecked(buf));
                    ret = len;
                } else {
                    ret = u64::MAX; // unsupported fd
                }
            }
            _ => {
                ret = u64::MAX; // unknown syscall
            }
        }
        debug!("returning {ret} from syscall_main");
        asm!("mov rax, {}", in(reg) ret);
    }
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
