#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct SdtHeader {
    pub signature: super::Signature,
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SdtHeader {
    pub fn verify(&self) -> bool {
        self.as_bytes()
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte))
            .eq(&0) // looks nicer than == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                (self as *const SdtHeader).cast::<u8>(),
                self.length as usize,
            )
        }
    }
}

impl core::fmt::Debug for SdtHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let len = self.length;
        let rev = self.oem_revision;
        let cid = self.creator_id;
        let cr = self.creator_revision;
        f.debug_struct("SdtHeader")
            .field("signature", &self.signature)
            .field("length", &len)
            .field("revision", &self.revision)
            .field("checksum", &self.checksum)
            .field(
                "oem_id",
                &str::from_utf8(&self.oem_id).unwrap_or("<UNKNOWN>"),
            )
            .field(
                "oem_table_id",
                &str::from_utf8(&self.oem_table_id).unwrap_or("<UNKNOWN>"),
            )
            .field("oem_revision", &rev)
            .field("creator_id", &cid)
            .field("creator_revision", &cr)
            .finish()
    }
}
