//! Multi-level page table structure for x86_64 architecture.
//!
//! This module provides the `PageTable` type which represents a single 4KB page table
//! containing 512 entries. Page tables are used in multi-level paging to translate
//! virtual addresses to physical addresses.

use super::entry::PageTableEntry;
use super::index::PageTableIndex;
use core::ops::{Index, IndexMut};

/// A single level in the x86_64 paging hierarchy.
///
/// A page table is a 4KB-aligned structure containing 512 entries (8 bytes each).
/// Each entry points to either the next level of paging tables or directly to a physical page.
///
/// The paging hierarchy:
/// - PML4 (Level 0): Indexes into PDPT tables
/// - PDPT (Level 1): Indexes into PDT tables
/// - PDT (Level 2): Indexes into PT tables
/// - PT (Level 3): Indexes into actual pages
///
/// # Memory Layout
/// - Size: 4096 bytes (4 KB)
/// - Alignment: 4096 bytes
/// - Entries: 512 × 8-byte entries
///
/// # Example
/// ```
/// let mut page_table = PageTable::new_zeroed();
/// let index = PageTableIndex::new(virt_addr, 3);
/// page_table[index].set_addr(phys_addr);
/// page_table[index].add_flags(PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
/// ```
#[repr(C, align(4096))]
#[derive(Clone, Debug)]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// Creates a new zeroed page table with all entries cleared.
    ///
    /// All entries will have address 0 and no flags set.
    ///
    /// # Returns
    /// A new, empty page table
    #[inline(always)]
    pub const fn new_zeroed() -> Self {
        Self {
            entries: [PageTableEntry::new_zeroed(); 512],
        }
    }

    /// Clears all entries in this page table, setting them to zero.
    ///
    /// After calling this, all entries will be unused and the page table
    /// will not map any addresses.
    pub fn zero(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.set_unused();
        }
    }

    /// Returns an iterator over all entries in this page table.
    ///
    /// # Returns
    /// An immutable iterator yielding references to each entry
    pub fn iter<'a>(&'a self) -> core::slice::Iter<'a, PageTableEntry> {
        self.entries.iter()
    }

    /// Returns a mutable iterator over all entries in this page table.
    ///
    /// # Returns
    /// A mutable iterator yielding mutable references to each entry
    pub fn iter_mut<'a>(&'a mut self) -> core::slice::IterMut<'a, PageTableEntry> {
        self.entries.iter_mut()
    }

    pub fn get_table(&self, index: usize) -> &PageTable {
        &self.entries[index].get_table()
    }

    /// Returns a mutable reference to the page table pointed to by the entry at `index`.
    ///
    /// # Safety
    /// The caller must ensure the entry is present and points to a valid page table.
    pub fn get_table_mut(&mut self, index: usize) -> &mut PageTable {
        self.entries[index].get_table_mut()
    }
}

/// Index a page table by raw index to get an entry.
impl Index<usize> for PageTable {
    type Output = PageTableEntry;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

/// Mutably index a page table by raw index to modify an entry.
impl IndexMut<usize> for PageTable {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

/// Index a page table by `PageTableIndex` to get an entry.
impl Index<PageTableIndex> for PageTable {
    type Output = PageTableEntry;

    #[inline(always)]
    fn index(&self, index: PageTableIndex) -> &Self::Output {
        &self.entries[index.index()]
    }
}

/// Mutably index a page table by `PageTableIndex` to modify an entry.
impl IndexMut<PageTableIndex> for PageTable {
    #[inline(always)]
    fn index_mut(&mut self, index: PageTableIndex) -> &mut Self::Output {
        &mut self.entries[index.index()]
    }
}

/// Immutable iteration over a page table yields references to entries.
impl<'a> IntoIterator for &'a PageTable {
    type Item = &'a PageTableEntry;
    type IntoIter = core::slice::Iter<'a, PageTableEntry>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// Mutable iteration over a page table yields mutable references to entries.
impl<'a> IntoIterator for &'a mut PageTable {
    type Item = &'a mut PageTableEntry;
    type IntoIter = core::slice::IterMut<'a, PageTableEntry>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter_mut()
    }
}
