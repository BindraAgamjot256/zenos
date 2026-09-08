use bitflags::bitflags;

bitflags! {
    /// Permissions and memory attributes for mapped pages.
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryType: u32 {
        /// The page is readable.
        const READABLE = 1 << 0;

        /// The page is writable.
        const WRITABLE = 1 << 1;

        /// The page is executable.
        const EXECUTABLE = 1 << 2;

        /// The page is accessible from user mode.
        const USER_ACCESSIBLE = 1 << 3;

        /// The page is global and should not be flushed from the TLB when CR3 changes.
        const GLOBAL = 1 << 4;

        /// The page is not cached.
        const NO_CACHE = 1 << 5;

        /// The page uses write-through caching.
        const WRITE_THROUGH = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    OutOfMem,
    Uninit,
}
