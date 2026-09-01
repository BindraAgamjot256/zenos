use super::AcpiTable;
use super::header::SdtHeader;
use core::marker::PhantomData;
use core::ptr::NonNull;

/// XSDT containing pointers to ACPI system description tables.
#[derive(Debug)]
#[repr(C, packed)]
pub struct Xsdt {
    pub header: SdtHeader,
}

impl Xsdt {
    /// Iterate over the tables referenced by this XSDT.
    pub fn iter<'a>(&'a self) -> XsdtTableIter<'a> {
        XsdtTableIter {
            xsdt: self as *const _ as *mut Xsdt,
            index: 0,
            phantom: PhantomData,
        }
    }

    /// Find the first table matching `T::SIG`.
    pub fn find_table<T: AcpiTable>(&self) -> Option<&T> {
        self.find_tables().next()
    }

    /// Find all tables matching `T::SIG`.
    pub fn find_tables<'a, T: AcpiTable + 'a>(&'a self) -> impl Iterator<Item = &'a T> {
        self.iter()
            .filter(|table| unsafe { table.as_ref().signature == T::SIG })
            .map(|table| unsafe { &*(table.as_ptr() as *mut T) })
    }
}

/// Iterator over the table entries in an XSDT.
pub struct XsdtTableIter<'a> {
    xsdt: *mut Xsdt,
    index: usize,

    // Tie the iterator lifetime to the XSDT it was created from.
    phantom: PhantomData<&'a ()>,
}

impl<'a> Iterator for XsdtTableIter<'a> {
    type Item = NonNull<SdtHeader>;

    fn next(&mut self) -> Option<Self::Item> {
        let header_size = core::mem::size_of::<SdtHeader>();
        let total_length = unsafe { (*self.xsdt).header.length } as usize;

        if total_length < header_size {
            return None;
        }

        // XSDT contains 64-bit (8-byte) pointers
        let total_entries = (total_length - header_size) / 8;
        if self.index >= total_entries {
            return None;
        }

        // Get a pointer to the start of the u64 array (just past the header)
        unsafe {
            let base_ptr = (self.xsdt).add(1) as *const u64;

            // read unaligned since xsdt is aligned to 0x1, while u64 requires 0x4
            let table_phys_addr = base_ptr.add(self.index).read_unaligned();
            self.index += 1;
            let table_virt_addr =
                (table_phys_addr as usize + crate::arch::mem::get_phys_offset()) as *mut SdtHeader;
            let item = NonNull::new(table_virt_addr)?;
            let header = &*item.as_ptr();
            if !header.verify() {
                log::info!("Invalid SDT header at 0x{:x}", table_phys_addr);
                log::info!("SDT: {:?}", header);
            }

            Some(item)
        }
    }
}
