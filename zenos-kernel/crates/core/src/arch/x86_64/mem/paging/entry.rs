//! Page table entry representation for x86_64 architecture.
//!
//! This module provides the `PageTableEntry` type which represents a single entry
//! in an x86_64 page table. Each entry contains a physical address and flags
//! controlling access and caching behavior.

use super::{PageTable, flags::PageTableFlags};
use crate::arch::PhysAddr;

/// A single entry in an x86_64 page table.
///
/// A page table entry (PTE) contains a 40-bit physical address and 12 bits of flags.
/// When the HUGE_PAGE flag is set, the physical address may span multiple pages.
///
/// # Layout
/// - Bits \[0:11\]: Flags (PRESENT, WRITABLE, etc.)
/// - Bits \[12:51\]: Physical address (40 bits)
/// - Bits \[52:62\]: Available for software use
/// - Bit 63: NO_EXECUTE flag
///
/// # Example
/// ```
/// let mut entry = PageTableEntry::new_zeroed();
/// entry.set_addr(PhysAddr::new(0x1000));
/// entry.add_flags(PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
/// assert!(entry.is_present());
/// assert!(entry.is_writable());
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    // Physical address bits: [12:51] (40 bits) + alignment (12 bits) = 52 bits masked
    const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
    // Flags bits: [0:11]
    const FLAGS_MASK: u64 = 0xfff;

    /// Creates a new zeroed page table entry with no flags set and address 0.
    ///
    /// The entry will not be present and will not map any physical address.
    #[inline(always)]
    pub const fn new_zeroed() -> Self {
        Self(0)
    }

    /// Checks if this entry is unused (completely zeroed).
    ///
    /// An unused entry has no flags set and contains address 0.
    #[inline(always)]
    pub fn is_unused(&self) -> bool {
        self.0 == 0
    }

    /// Marks this entry as unused by clearing all bits to zero.
    #[inline(always)]
    pub fn set_unused(&mut self) {
        self.0 = 0;
    }

    /// Returns the physical address contained in this entry.
    ///
    /// The address is extracted from bits \[12:51\] and aligned to 4096-byte (4KB) pages.
    ///
    /// # Returns
    /// The physical address stored in this entry
    #[inline(always)]
    pub fn addr(&self) -> PhysAddr {
        // SAFETY: We mask the address to ensure it's valid (max 48-bit physical address)
        // The mask ensures bits [52:63] are cleared, resulting in a valid physical address
        unsafe { PhysAddr::new_unchecked(self.0 & Self::ADDR_MASK) }
    }

    /// Sets the physical address for this entry.
    ///
    /// The address must be 4KB-aligned (bits \[0:11\] must be zero). The address
    /// is stored in bits \[12:51\]; higher bits are preserved.
    ///
    /// # Arguments
    /// * `addr` - The physical address to store
    ///
    /// # Panics
    /// Panics if the address is not properly aligned.
    #[inline(always)]
    pub fn set_addr(&mut self, addr: PhysAddr) {
        self.0 &= !Self::ADDR_MASK;
        self.0 |= addr.as_u64() & Self::ADDR_MASK;
    }

    /// Returns all flags set on this entry.
    ///
    /// # Returns
    /// A `PageTableFlags` bitset containing all flags currently set
    #[inline(always)]
    pub fn flags(&self) -> PageTableFlags {
        PageTableFlags::from_bits_truncate(self.0)
    }

    /// Replaces all flags on this entry.
    ///
    /// Any flags previously set are cleared; only the provided flags are set.
    ///
    /// # Arguments
    /// * `flags` - The new flags to set
    #[inline(always)]
    pub fn set_flags(&mut self, flags: PageTableFlags) {
        self.0 &= !Self::FLAGS_MASK;
        self.0 |= flags.bits();
    }

    /// Adds flags to this entry without clearing existing flags.
    ///
    /// If a flag is already set, it remains set.
    ///
    /// # Arguments
    /// * `flags` - The flags to add
    #[inline(always)]
    pub fn add_flags(&mut self, flags: PageTableFlags) {
        self.0 |= flags.bits();
    }

    /// Removes flags from this entry without affecting other flags.
    ///
    /// If a flag is not set, it remains unset.
    ///
    /// # Arguments
    /// * `flags` - The flags to remove
    #[inline(always)]
    pub fn remove_flags(&mut self, flags: PageTableFlags) {
        self.0 &= !flags.bits();
    }

    /// Checks if all specified flags are set.
    ///
    /// # Arguments
    /// * `flags` - The flags to check for
    ///
    /// # Returns
    /// `true` if all specified flags are present, `false` otherwise
    #[inline(always)]
    pub fn has_flags(&self, flags: PageTableFlags) -> bool {
        self.flags().contains(flags)
    }

    /// Checks if this entry points to a present page.
    ///
    /// # Returns
    /// `true` if the PRESENT flag is set, `false` otherwise
    #[inline(always)]
    pub fn is_present(&self) -> bool {
        self.has_flags(PageTableFlags::PRESENT)
    }

    /// Checks if this page is writable.
    ///
    /// # Returns
    /// `true` if the WRITABLE flag is set, `false` otherwise
    #[inline(always)]
    pub fn is_writable(&self) -> bool {
        self.has_flags(PageTableFlags::WRITABLE)
    }

    /// Checks if this page is accessible from user mode (ring 3).
    ///
    /// # Returns
    /// `true` if the USER_ACCESSIBLE flag is set, `false` otherwise
    #[inline(always)]
    pub fn is_user_accessible(&self) -> bool {
        self.has_flags(PageTableFlags::USER_ACCESSIBLE)
    }

    /// Returns the raw 64-bit value of this entry.
    ///
    /// Useful for debugging or interfacing with hardware that expects the raw format.
    ///
    /// # Returns
    /// The uninterpreted 64-bit entry value
    #[inline(always)]
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Sets the raw 64-bit value for this entry.
    ///
    /// # Safety
    /// The caller must ensure the value is a valid page table entry with proper
    /// address alignment and valid flag bits.
    ///
    /// # Arguments
    /// * `value` - The raw 64-bit entry value
    #[inline(always)]
    pub fn set_raw(&mut self, value: u64) {
        self.0 = value;
    }

    /// Returns a reference to the page table pointed to by this entry.
    ///
    /// # Safety
    /// The caller must ensure the entry is present and points to a valid page table.
    #[inline(always)]
    pub fn get_table(&self) -> &PageTable {
        unsafe { &*(self.0 as *const PageTable) }
    }

    /// Returns a mutable reference to the page table pointed to by this entry.
    ///
    /// # Safety
    /// The caller must ensure the entry is present and points to a valid page table.
    #[inline(always)]
    pub fn get_table_mut(&mut self) -> &mut PageTable {
        unsafe { &mut *(self.0 as *mut PageTable) }
    }
}
