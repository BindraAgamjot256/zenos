use crate::arch::{PhysAddr, VirtAddr};
use core::arch::asm;

pub struct CR3;

impl CR3 {
    #[inline]
    pub fn read_raw() -> (PhysAddr, u16) {
        let value: u64;
        unsafe {
            asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
        }

        let addr = PhysAddr::new(value & 0x_000f_ffff_ffff_f000);
        (addr, (value & 0xFFF) as u16)
    }
}

pub struct CR2;

impl CR2 {
    #[inline]
    pub fn read_raw() -> u64 {
        let value: u64;
        unsafe {
            asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
        }
        value
    }

    pub fn read() -> Option<VirtAddr> {
        let value: u64 = Self::read_raw();
        VirtAddr::try_new(value)
    }
}
