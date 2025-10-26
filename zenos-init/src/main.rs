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
    // Minimal init process that loops forever
    let buf = b"hello world\n";
    let len = buf.len();
    let ret: isize;
    unsafe {
        asm!(
        "syscall",
        in("rax") 1usize,            // syscall number: write
        in("rdi") 1usize,            // fd = 1 (stdout)
        in("rsi") buf.as_ptr(),      // buffer pointer
        in("rdx") len,               // buffer length
        lateout("rax") ret,          // syscall return -> rax
        out("rcx") _,                // syscall clobbers rcx
        out("r11") _,                // syscall clobbers r11
        options(nostack),            // we don't touch the stack here
        );
    }
    loop {}
}
