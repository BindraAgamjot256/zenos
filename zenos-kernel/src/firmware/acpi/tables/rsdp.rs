use core::ptr::{self, NonNull};

use crate::firmware::acpi::tables::Xsdt;

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

impl Rsdp {
    const SIGNATURE: [u8; 8] = *b"RSD PTR ";
    const LEN_SHORT: usize = 20;
    const LEN_FULL: usize = 36;

    pub fn from_address(address: usize) -> Option<Self> {
        let ptr = (crate::arch::mem::get_phys_offset() + address) as *const Self;

        let maybersdp = unsafe { core::ptr::read_unaligned(ptr) };

        if !maybersdp.validate() {
            return None;
        }

        Some(maybersdp)
    }

    pub fn validate(&self) -> bool {
        if self.signature != Self::SIGNATURE {
            return false;
        }

        let base_ptr = self as *const Self as *const u8;

        // ACPI 1.0 Checksum (First 20 bytes) - Mandatory for ALL revisions
        let mut short_sum: u8 = 0;
        for i in 0..Self::LEN_SHORT {
            unsafe {
                short_sum = short_sum.wrapping_add(base_ptr.add(i).read());
            }
        }
        if short_sum != 0 {
            return false;
        }

        // ACPI 2.0+ Extended Checksum (All 36 bytes)
        if self.revision >= 2 {
            let mut full_sum: u8 = 0;
            for i in 0..Self::LEN_FULL {
                unsafe {
                    full_sum = full_sum.wrapping_add(base_ptr.add(i).read());
                }
            }
            if full_sum != 0 {
                return false;
            }
        }

        true
    }
    pub fn revision(&self) -> u8 {
        self.revision
    }
    pub fn rsdt_address(&self) -> Option<NonNull<()>> {
        if self.revision < 2 {
            None
        } else {
            NonNull::new(ptr::with_exposed_provenance_mut(
                self.rsdt_address as usize + crate::arch::mem::get_phys_offset(),
            ))
        }
    }
    pub fn xsdt_address(&self) -> Option<NonNull<super::sdt::Xsdt>> {
        if self.revision < 2 || self.xsdt_address == 0 {
            None
        } else {
            let ptr = NonNull::new(ptr::with_exposed_provenance_mut(
                self.xsdt_address as usize + crate::arch::mem::get_phys_offset(),
            ));
            if let Some(ptr) = ptr {
                let tptr: &Xsdt = unsafe { ptr.as_ref() };
                if tptr.header.verify() {
                    return Some(ptr);
                } else {
                    return None;
                }
            }
            None
        }
    }
}

impl core::fmt::Debug for Rsdp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let raddr = self.rsdt_address;
        let len = self.length;
        let xaddr = self.xsdt_address;
        f.debug_struct("Rsdp")
            .field(
                "signature",
                &str::from_utf8(&self.signature).unwrap_or("<UNKNOWN>"),
            )
            .field("checksum", &self.checksum)
            .field("OEM", &str::from_utf8(&self.oem_id).unwrap_or("<UNKNOWN>"))
            .field("revision", &self.revision)
            .field("rsdt_address", &raddr)
            .field("length", &len)
            .field("xsdt_address", &xaddr)
            .field("xsdt_checksum", &self.extended_checksum)
            .field("valid", &self.validate())
            .finish()
    }
}
