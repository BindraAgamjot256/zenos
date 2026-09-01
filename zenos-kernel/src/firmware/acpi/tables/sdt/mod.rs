mod header;
mod hpet;
mod madt;
mod xsdt;

pub use hpet::Hpet;
pub use madt::{LapicEntryFlags, Madt, MadtEntry};
pub use xsdt::Xsdt;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Signature([u8; 4]);

impl core::fmt::Debug for Signature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", str::from_utf8(&self.0).unwrap_or("<UNKNOWN>"))
    }
}

pub trait AcpiTable {
    const SIG: Signature;
}
