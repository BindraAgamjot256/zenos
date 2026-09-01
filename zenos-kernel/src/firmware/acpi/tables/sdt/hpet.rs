use super::{super::address::Address, AcpiTable, Signature, header::SdtHeader};

#[repr(C, packed)]
#[derive(Debug)]
pub struct Hpet {
    pub header: SdtHeader,
    pub event_timer_id: u32,
    pub addr: Address,
    pub number: u8,
    pub tick_unit: u16,
    pub flags: u8,
}

impl AcpiTable for Hpet {
    const SIG: Signature = Signature(*b"HPET");
}
