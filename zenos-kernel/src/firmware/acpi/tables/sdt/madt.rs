use core::mem::size_of;

use super::{AcpiTable, Signature, header::SdtHeader};

#[repr(C, packed)]
#[derive(Debug)]
pub struct Madt {
    pub header: SdtHeader,
    pub lapic_addr: u32,
    pub flags: u32,
}

impl AcpiTable for Madt {
    const SIG: Signature = Signature(*b"APIC");
}

impl Madt {
    pub fn into_iter(&self) -> MadtIterator {
        MadtIterator {
            madt: self,
            len: self.header.length as usize,
            cursor: size_of::<Madt>(),
        }
    }
}

pub struct MadtIterator {
    madt: *const Madt,
    len: usize,
    cursor: usize,
}

impl Iterator for MadtIterator {
    type Item = MadtEntry;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            // No more entries.
            if self.cursor >= self.len {
                return None;
            }

            // Make sure we can read the entry header.
            if self.len - self.cursor < size_of::<EntryHeader>() {
                return None;
            }

            let entry_ptr = self
                .madt
                .cast::<u8>()
                .add(self.cursor)
                .cast::<EntryHeader>();

            let entry = entry_ptr.read_unaligned();

            let entry_len = entry.length as usize;

            // Every entry must contain at least its type and length.
            if entry_len < size_of::<EntryHeader>() {
                return None;
            }

            // Don't read beyond the MADT.
            if entry_len > self.len - self.cursor {
                return None;
            }

            match entry.ftype {
                0 => {
                    // Local APIC entries are exactly 8 bytes.
                    if entry_len != 8 {
                        return None;
                    }

                    let ptr = entry_ptr.add(1).cast::<u8>();

                    let processor_id = ptr.read_unaligned();
                    let apic_id = ptr.add(1).read_unaligned();

                    let flags = {
                        let raw = ptr.add(2).cast::<u32>().read_unaligned();
                        match raw {
                            0 => LapicEntryFlags::Disabled,
                            1 => LapicEntryFlags::Enabled,
                            2 => LapicEntryFlags::HotPlugable,
                            _ => LapicEntryFlags::Invalid,
                        }
                    };

                    self.cursor += entry_len;

                    Some(MadtEntry::LocalApic {
                        processor_id,
                        apic_id,
                        flags,
                    })
                }

                1 => {
                    if entry_len != 12 {
                        return None;
                    }

                    let ptr = entry_ptr.cast::<u8>();

                    let io_apic_id = ptr.add(2).read_unaligned();
                    let io_apic_addr = ptr.add(4).cast::<u32>().read_unaligned();
                    let gsi_base = ptr.add(8).cast::<u32>().read_unaligned();

                    self.cursor += entry_len;

                    Some(MadtEntry::IoApic {
                        io_apic_id,
                        _resv: 0,
                        io_apic_addr,
                        gsi_base,
                    })
                }

                2 => {
                    // IO APIC ISR override
                    if entry_len != 10 {
                        return None;
                    }

                    let ptr = entry_ptr.cast::<u8>();
                    let bus = ptr.add(2).read_unaligned();
                    let irq = ptr.add(3).read_unaligned();
                    let gsi = ptr.add(4).cast::<u32>().read_unaligned();
                    let flags = ptr.add(8).cast::<u16>().read_unaligned();

                    self.cursor += entry_len;
                    Some(MadtEntry::IoApicIsrOverride {
                        _bus: bus,
                        source: irq,
                        gsi,
                        flags,
                    })
                }

                3 => {
                    // IO APIC NMI
                    if entry_len != 10 {
                        return None;
                    }

                    let ptr = entry_ptr.cast::<u8>();
                    let src = ptr.add(2).read_unaligned();
                    let flags = ptr.add(4).cast::<u16>().read_unaligned();
                    let gsi = ptr.add(6).cast::<u32>().read_unaligned();

                    self.cursor += entry_len;
                    Some(MadtEntry::IoApicNmi {
                        nmi_source: src,
                        _resv: 0,
                        flags,
                        gsi,
                    })
                }

                4 => {
                    // LAPIC NMI
                    if entry_len != 8 {
                        return None;
                    }

                    let ptr = entry_ptr.cast::<u8>();
                    let pid = ptr.add(2).read_unaligned();
                    let flags = ptr.add(3).cast::<u16>().read_unaligned();
                    let lint = ptr.add(5).read_unaligned();

                    self.cursor += entry_len;
                    Some(MadtEntry::LocalApicNmi {
                        processor_id: pid,
                        flags,
                        lint,
                    })
                }

                _ => {
                    // Unknown entry type. Skip it rather than treating it
                    // as the end of the MADT.
                    self.cursor += entry_len;
                    log::warn!("unknown entry type: {}", entry.ftype);
                    self.next()
                }
            }
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct EntryHeader {
    ftype: u8,
    length: u8,
}

#[derive(Debug)]
#[repr(C)]
pub enum MadtEntry {
    LocalApic {
        processor_id: u8,
        apic_id: u8,
        flags: LapicEntryFlags,
    },
    IoApic {
        io_apic_id: u8,
        _resv: u8,
        io_apic_addr: u32,
        gsi_base: u32,
    },
    IoApicIsrOverride {
        _bus: u8,
        source: u8,
        gsi: u32,
        flags: u16,
    },

    // NMIs are currently unused
    #[allow(dead_code)]
    IoApicNmi {
        nmi_source: u8,
        _resv: u8,
        flags: u16,
        gsi: u32,
    },
    #[allow(dead_code)]
    LocalApicNmi {
        processor_id: u8,
        flags: u16,
        lint: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(u32)]
pub enum LapicEntryFlags {
    Disabled = 0x0,
    Enabled = 0x1,
    HotPlugable = 1 << 1,
    Invalid = 0b11,
}
