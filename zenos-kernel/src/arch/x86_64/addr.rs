//! Physical and virtual address types for x86_64 architecture.
//!
//! This module provides validated address types that enforce architectural constraints:
//! - `PhysAddr`: Validates that addresses fit within the 48-bit physical address space
//! - `VirtAddr`: Validates that addresses are canonical (proper sign extension)
//!
//! Both types provide conversion methods, arithmetic operations, and display formatting.

use core::fmt;
use core::ops::{Add, Deref, Div, Mul, Sub};

/// A validated 48-bit physical address (0 to 2^48 - 1).
///
/// x86_64 systems support up to 48 bits of physical addressing (256 TiB of physical memory).
/// This type ensures that only valid physical addresses can be created.
///
/// # Example
/// ```
/// let addr = PhysAddr::new(0x1000); // 4 KiB physical address
/// assert_eq!(addr.as_u64(), 0x1000);
/// ```
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct PhysAddr(u64);

/// A validated canonical x86_64 virtual address.
///
/// x86_64 virtual addresses must be "canonical": bits \[0:47\] can be any value,
/// but bits \[48:63\] must match bit 47 (sign extension). This ensures proper addressing
/// and prevents invalid addresses from being used.
///
/// # Canonical Form
/// For an address to be canonical:
/// - If bit 47 is 0, bits \[48:63\] must all be 0 (kernel space)
/// - If bit 47 is 1, bits \[48:63\] must all be 1 (user space)
///
/// # Example
/// ```
/// let addr = VirtAddr::new(0x00007f7f7f7f7f00); // User-space canonical address
/// assert!(addr.as_u64() < (1u64 << 48));
/// ```
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct VirtAddr(u64);

// ============================================================================
// PhysAddr Implementation
// ============================================================================

impl PhysAddr {
    /// Maximum valid physical address (2^48 - 1)
    const MAX_PHYS: u64 = (1u64 << 48) - 1;

    /// Checks if the given address is a valid physical address (fits in 48 bits).
    #[inline]
    fn is_valid_phys(addr: u64) -> bool {
        addr <= Self::MAX_PHYS
    }

    /// Creates a new `PhysAddr`, panicking if the address is invalid.
    ///
    /// This method validates that the address fits within the 48-bit physical address space.
    /// If the address is invalid, this function panics with an error message.
    ///
    /// # Arguments
    /// * `addr` - The physical address to validate and wrap
    ///
    /// # Panics
    /// Panics if `addr` exceeds 2^48 - 1 (the maximum valid physical address)
    ///
    /// # Example
    /// ```
    /// let addr = PhysAddr::new(0x1000);
    /// // PhysAddr::new(0x1_0000_0000_0000); // Panics: exceeds 48 bits
    /// ```
    #[inline]
    #[track_caller]
    pub fn new(addr: u64) -> Self {
        Self::try_new(addr).expect("invalid physical address (exceeds 48 bits)")
    }

    /// Creates a new `PhysAddr` if the address is valid, otherwise returns `None`.
    ///
    /// This is the fallible version of `new()`. Returns `None` if the address
    /// exceeds 48 bits; otherwise returns a `Some(PhysAddr)`.
    ///
    /// # Arguments
    /// * `addr` - The physical address to validate
    ///
    /// # Returns
    /// `Some(PhysAddr)` if `addr` is valid, `None` otherwise
    #[inline]
    pub fn try_new(addr: u64) -> Option<Self> {
        if Self::is_valid_phys(addr) {
            Some(unsafe { Self::new_unchecked(addr) })
        } else {
            None
        }
    }

    /// Creates a new `PhysAddr` without validation.
    ///
    /// # Safety
    /// The caller must ensure that `addr` is a valid physical address (fits in 48 bits).
    /// If the address is invalid, subsequent operations may produce incorrect results
    /// or cause memory unsafety.
    #[inline]
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        PhysAddr(addr)
    }

    /// Returns the address as a `u64`.
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the address as a `usize`.
    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:012x}", self.0)
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr(0x{:012x})", self.0)
    }
}

// Arithmetic operations for PhysAddr
impl Add<u64> for PhysAddr {
    type Output = PhysAddr;

    #[inline]
    fn add(self, rhs: u64) -> PhysAddr {
        unsafe { PhysAddr::new_unchecked(self.0.wrapping_add(rhs)) }
    }
}

impl Sub<u64> for PhysAddr {
    type Output = PhysAddr;

    #[inline]
    fn sub(self, rhs: u64) -> PhysAddr {
        unsafe { PhysAddr::new_unchecked(self.0.wrapping_sub(rhs)) }
    }
}

impl Mul<u64> for PhysAddr {
    type Output = PhysAddr;

    #[inline]
    fn mul(self, rhs: u64) -> PhysAddr {
        unsafe { PhysAddr::new_unchecked(self.0.wrapping_mul(rhs)) }
    }
}

impl Div<u64> for PhysAddr {
    type Output = PhysAddr;

    #[inline]
    fn div(self, rhs: u64) -> PhysAddr {
        unsafe { PhysAddr::new_unchecked(self.0 / rhs) }
    }
}

// Distance between two physical addresses
impl Sub<PhysAddr> for PhysAddr {
    type Output = u64;

    #[inline]
    fn sub(self, rhs: PhysAddr) -> u64 {
        self.0.wrapping_sub(rhs.0)
    }
}

impl Deref for PhysAddr {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for PhysAddr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// ============================================================================
// VirtAddr Implementation
// ============================================================================

impl VirtAddr {
    /// Checks if the given address is canonical (bits 48-63 match bit 47).
    ///
    /// A canonical address has proper sign extension: if bit 47 is set, all of bits \[48:63\]
    /// must be set; if bit 47 is clear, all of bits \[48:63\] must be clear.
    #[inline]
    fn is_canonical(addr: u64) -> bool {
        let bit_47 = (addr >> 47) & 1;
        let upper_bits = addr >> 48;
        // All upper bits must match bit 47
        if bit_47 == 0 {
            upper_bits == 0
        } else {
            upper_bits == 0xFFFF
        }
    }

    /// Creates a new `VirtAddr`, panicking if the address is not canonical.
    ///
    /// This method validates that the address is canonical (has proper sign extension).
    /// If the address is not canonical, this function panics with an error message.
    ///
    /// # Arguments
    /// * `addr` - The virtual address to validate and wrap
    ///
    /// # Panics
    /// Panics if `addr` is not a canonical virtual address
    ///
    /// # Example
    /// ```
    /// let addr = VirtAddr::new(0x00007f7f7f7f7f00); // Valid user-space address
    /// // VirtAddr::new(0x0000800000000000); // Panics: not canonical
    /// ```
    #[inline]
    pub fn new(addr: u64) -> Self {
        Self::try_new(addr).expect("invalid virtual address (not canonical)")
    }

    /// Creates a new `VirtAddr` if the address is canonical, otherwise returns `None`.
    ///
    /// This is the fallible version of `new()`. Returns `None` if the address
    /// is not canonical; otherwise returns a `Some(VirtAddr)`.
    ///
    /// # Arguments
    /// * `addr` - The virtual address to validate
    ///
    /// # Returns
    /// `Some(VirtAddr)` if `addr` is canonical, `None` otherwise
    #[inline]
    pub fn try_new(addr: u64) -> Option<Self> {
        if Self::is_canonical(addr) {
            Some(unsafe { Self::new_unchecked(addr) })
        } else {
            None
        }
    }

    /// Creates a new `VirtAddr` without validation.
    ///
    /// # Safety
    /// The caller must ensure that `addr` is a canonical x86_64 virtual address.
    /// If the address is not canonical, subsequent operations may produce incorrect results.
    #[inline]
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        VirtAddr(addr)
    }

    /// Returns the address as a `u64`.
    ///
    /// # Returns
    /// The virtual address as a 64-bit unsigned integer
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the address as a `usize`.
    ///
    /// # Returns
    /// The virtual address as a `usize` (typically 64 bits on x86_64)
    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}", self.0)
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr(0x{:016x})", self.0)
    }
}

// Arithmetic operations for VirtAddr
impl Add<u64> for VirtAddr {
    type Output = VirtAddr;

    #[inline]
    fn add(self, rhs: u64) -> VirtAddr {
        unsafe { VirtAddr::new_unchecked(self.0.wrapping_add(rhs)) }
    }
}

impl Sub<u64> for VirtAddr {
    type Output = VirtAddr;

    #[inline]
    fn sub(self, rhs: u64) -> VirtAddr {
        unsafe { VirtAddr::new_unchecked(self.0.wrapping_sub(rhs)) }
    }
}

impl Mul<u64> for VirtAddr {
    type Output = VirtAddr;

    #[inline]
    fn mul(self, rhs: u64) -> VirtAddr {
        unsafe { VirtAddr::new_unchecked(self.0.wrapping_mul(rhs)) }
    }
}

impl Div<u64> for VirtAddr {
    type Output = VirtAddr;

    #[inline]
    fn div(self, rhs: u64) -> VirtAddr {
        unsafe { VirtAddr::new_unchecked(self.0 / rhs) }
    }
}

// Distance between two virtual addresses
impl Sub<VirtAddr> for VirtAddr {
    type Output = i64;

    #[inline]
    fn sub(self, rhs: VirtAddr) -> i64 {
        self.0.wrapping_sub(rhs.0) as i64
    }
}

impl Deref for VirtAddr {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for VirtAddr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
