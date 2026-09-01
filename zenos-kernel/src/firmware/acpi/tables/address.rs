#![allow(dead_code)]

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
    pub id: ID,
    pub reg_width: u8,
    pub reg_offset: u8,
    pub access: Access,
    pub address: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ID {
    SystemMemorySpace = 0x00,
    SystemIoSpace = 0x01,
    SystemPciSpace = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Access {
    Undefined = 0x00,
    ByteAccess = 0x01,
    WordAccess = 0x02,
    DwordAccess = 0x03,
    QwordAccess = 0x04,
}
