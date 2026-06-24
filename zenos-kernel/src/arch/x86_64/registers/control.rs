use crate::arch::PhysAddr;
use crate::arch::x86_64::mem::paging::Frame;
use crate::arch::x86_64::mem::paging::page::Size4K;
use core::arch::asm;

pub struct CR3;

impl CR3 {
    #[inline]
    pub fn read_raw() -> (Frame<Size4K>, u16) {
        let value: u64;
        unsafe {
            asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
        }

        let addr = PhysAddr::new(value & 0x_000f_ffff_ffff_f000);
        let frame = Frame::containing_address(addr.as_usize());
        (frame, (value & 0xFFF) as u16)
    }
}
