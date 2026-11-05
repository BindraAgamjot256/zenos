//! Page table index calculation for multi-level paging.
//!
//! This module provides utilities for calculating indices into page table levels
//! based on virtual addresses and paging levels.

use crate::arch::VirtAddr;

/// An index into a page table at a specific paging level.
///
/// This type encapsulates the 9-bit index that selects an entry within a page table.
/// Page tables in x86_64 have 512 entries, requiring 9 bits to index.
///
/// # Paging Levels
/// For 4-level paging:
/// - Level 0: PML4 (Page Map Level 4)
/// - Level 1: PDPT (Page Directory Pointer Table)
/// - Level 2: PDT (Page Directory Table)
/// - Level 3: PT (Page Table)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableIndex(u16);

impl PageTableIndex {
    /// Calculates the page table index for a given virtual address at a specific level.
    ///
    /// # Arguments
    /// * `addr` - The virtual address to extract the index from
    /// * `level` - The paging level (0-3 for 4-level paging) Level 0 extracts the highest 9 bits of the address
    ///
    /// # Returns
    /// The 9-bit index into the page table at the specified level
    ///
    /// # Example
    /// ```
    /// let addr = VirtAddr::new(0x00007f7f7f7f7f00);
    /// let index0 = PageTableIndex::new(addr, 0); // PML4 index
    /// let index3 = PageTableIndex::new(addr, 3); // PT index
    /// ```
    #[inline(always)]
    pub fn new(addr: VirtAddr, level: usize) -> Self {
        let shift = 12 + (3 - level) * 9; // DO NOT FUCKING REPLACE THIS WITH LEVEL. IT TOOK ME TWO FUCKING DAYS TO FIND A PAGING BUG HERE.
        let index = (addr.as_u64() >> shift) & 0o777;
        Self(index as u16)
    }

    /// Returns the index as a `usize`, suitable for array indexing.
    ///
    /// # Returns
    /// The page table index as a usize (0-511)
    #[inline(always)]
    pub fn index(&self) -> usize {
        self.0 as usize
    }
}
