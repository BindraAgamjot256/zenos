//! Page table entry flags for x86_64 architecture.
//!
//! This module defines the flags that can be set on page table entries to control
//! memory access permissions, caching behavior, and other page-level properties.

bitflags::bitflags! {
    /// Flags that control access and behavior of a page table entry.
    ///
    /// These flags correspond to the bits in an x86_64 page table entry (PTE).
    /// Multiple flags can be combined together using bitwise OR operations.
    ///
    /// # Architecture Details
    /// - Bits [0:11] are used for flags
    /// - Bits [12:51] contain the physical address
    /// - Bits [52:62] are available for software use
    /// - Bit 63 is the NO_EXECUTE (NX) flag
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageTableFlags: u64 {
        /// Page is currently present in physical memory.
        /// If not set, a page fault occurs when this entry is accessed.
        const PRESENT         = 1 << 0;

        /// Page is writable. If not set, writes to the page cause a page fault.
        const WRITABLE        = 1 << 1;

        /// Page is accessible from user-mode (ring 3).
        /// If not set, only kernel-mode (rings 0-2) can access the page.
        const USER_ACCESSIBLE = 1 << 2;

        /// Write-through cache policy. Updates to the page are written to memory immediately.
        /// If not set, write-back caching is used.
        const WRITE_THROUGH   = 1 << 3;

        /// Cache disabled. The page is not cached; all accesses go to memory.
        /// Useful for memory-mapped I/O.
        const NO_CACHE        = 1 << 4;

        /// Page has been accessed (read or written).
        /// The CPU sets this flag; software can clear it to track access patterns.
        const ACCESSED        = 1 << 5;

        /// Page has been written to (dirty bit).
        /// The CPU sets this flag on writes; software can clear it.
        const DIRTY           = 1 << 6;

        /// Entry points to a huge page (2MB for PD, 1GB for PDPT).
        /// Cannot be set on PML4 entries.
        const HUGE_PAGE       = 1 << 7;

        /// Global page. The TLB entry for this page is not invalidated when CR3 is loaded.
        /// Used for kernel pages that are present in all page tables.
        const GLOBAL          = 1 << 8;

        /// Execute disable. If set, instruction fetches from this page cause a page fault.
        /// Requires the NXE bit in EFER MSR to be set.
        const NO_EXECUTE      = 1 << 63;
    }
}
