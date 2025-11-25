#![no_std]
#![no_main]
use core::arch::asm;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // In userland, just loop forever for now
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main();
    unreachable!()
}

fn main() {
    // Minimal init process that demonstrates sys_write (1) and sys_read (0)
    let buf = b"type something and press enter...\n";
    let len = buf.len();
    let _ret: isize;
    unsafe {
        asm!(
        "int 0x80",
        in("rax") 1usize,            // syscall number: write
        in("rdi") 1usize,            // fd = 1 (stdout)
        in("rsi") buf.as_ptr(),      // buffer pointer
        in("rdx") len,               // buffer length
        lateout("rax") _ret,         // syscall return -> rax
        out("rcx") _,                // syscall clobbers rcx
        out("r11") _,                // syscall clobbers r11
        options(nostack),            // we don't touch the stack here
        );
    }

    // Read a line from stdin (fd = 0) into a small buffer
    let mut inbuf = [0u8; 64];
    let mut bytes_read: isize = 0;
    unsafe {
        asm!(
        "int 0x80",
        in("rax") 0usize,                // syscall number: read
        in("rdi") 0usize,                // fd = 0 (stdin)
        in("rsi") inbuf.as_mut_ptr(),    // buffer pointer
        in("rdx") inbuf.len(),           // buffer length
        lateout("rax") bytes_read,       // syscall return -> rax
        out("rcx") _,                    // syscall clobbers rcx
        out("r11") _,                    // syscall clobbers r11
        options(nostack),                // we don't touch the stack here
        );
    }

    let len_to_write = if bytes_read > 0 {
        bytes_read as usize
    } else {
        0
    };

    if len_to_write > 0 {
        // Echo what we read back to stdout
        let _ret2: isize;
        unsafe {
            asm!(
            "int 0x80",
            in("rax") 1usize,                 // syscall number: write
            in("rdi") 1usize,                 // fd = 1 (stdout)
            in("rsi") inbuf.as_ptr(),         // buffer pointer
            in("rdx") len_to_write,           // buffer length
            lateout("rax") _ret2,             // syscall return -> rax
            out("rcx") _,                     // syscall clobbers rcx
            out("r11") _,                     // syscall clobbers r11
            options(nostack),                 // we don't touch the stack here
            );
        }
    }

    loop {}
}
