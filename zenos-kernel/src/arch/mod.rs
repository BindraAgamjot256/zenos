#![allow(dead_code)]
#![doc(hidden)]
// here be dragons. run away.

#[inline(always)]
pub(crate) unsafe fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub(crate) unsafe fn inb(port: u16) -> u8 {
    unsafe {
        let value: u8;
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
        value
    }
}
#[inline(always)]
pub(crate) unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!(
    "out dx, ax",
    in("dx") port,
    in("ax") val,
    options(nomem, nostack, preserves_flags)
    );
}

#[inline(always)]
pub(crate) unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
    "in eax, dx",
    out("eax") value,
    in("dx") port,
    options(nomem, nostack, preserves_flags)
    );
    value
}

#[inline(always)]
pub(crate) unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!(
    "out dx, eax",
    in("dx") port,
    in("eax") val,
    options(nomem, nostack, preserves_flags)
    );
}
