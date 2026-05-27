//! Low-level I/O port access for x86_64 architecture.
//!
//! This module provides safe abstractions for reading from and writing to I/O ports,
//! which are essential for hardware communication. The module uses the type system to
//! enforce access restrictions (read-only, write-only, or read-write).
//!
//! # Architecture Details
//! x86_64 uses the IN and OUT instructions to access I/O ports. This module provides:
//! - `PortIO`: Trait for types that can be read/written to ports (u8, u16, u32)
//! - `Port<T, Mode>`: Generic port wrapper with access control via the type system
//! - Capability markers: `ReadOnly`, `WriteOnly`, `ReadWrite`
//! - Convenience type aliases: `ReadOnlyPort<T>`, `WriteOnlyPort<T>`, `ReadWritePort<T>`

use core::marker::PhantomData;

/// Low-level I/O port read instruction (IN instruction, byte).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to read from.
/// This executes the x86_64 `IN AL, DX` instruction.
#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
/// Low-level I/O port read instruction (IN instruction, word).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to read from.
/// This executes the x86_64 `IN AX, DX` instruction.
#[inline(always)]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
/// Low-level I/O port read instruction (IN instruction, double word).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to read from.
/// This executes the x86_64 `IN EAX, DX` instruction.
#[inline(always)]
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
/// Low-level I/O port write instruction (OUT instruction, byte).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to write to.
/// This executes the x86_64 `OUT DX, AL` instruction.
#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}
/// Low-level I/O port write instruction (OUT instruction, word).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to write to.
/// This executes the x86_64 `OUT DX, AX` instruction.
#[inline(always)]
unsafe fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}
/// Low-level I/O port write instruction (OUT instruction, double word).
///
/// # Safety
/// The caller must ensure that the port is valid and safe to write to.
/// This executes the x86_64 `OUT DX, EAX` instruction.
#[inline(always)]
unsafe fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Trait for types that can be read from and written to I/O ports.
///
/// This trait is implemented for `u8`, `u16`, and `u32`, representing the three
/// standard data widths supported by x86_64 I/O instructions (IN/OUT).
///
/// # Safety
/// All methods on this trait are unsafe because I/O port access can have
/// arbitrary side effects on hardware.
pub trait PortIO: Sized {
    /// Reads a value from the specified I/O port.
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - The port number is valid for the hardware
    /// - Reading from the port is safe and will not cause undefined behavior
    /// - The port is properly initialized
    unsafe fn read(port: u16) -> Self;

    /// Writes a value to the specified I/O port.
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - The port number is valid for the hardware
    /// - Writing to the port is safe and will not cause undefined behavior
    /// - The port is properly initialized
    unsafe fn write(port: u16, value: Self);
}

impl PortIO for u8 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inb(port) }
    }
    unsafe fn write(port: u16, value: Self) {
        unsafe { outb(port, value) }
    }
}

impl PortIO for u16 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inw(port) }
    }
    unsafe fn write(port: u16, value: Self) {
        unsafe { outw(port, value) }
    }
}

impl PortIO for u32 {
    unsafe fn read(port: u16) -> Self {
        unsafe { inl(port) }
    }
    unsafe fn write(port: u16, value: Self) {
        unsafe { outl(port, value) }
    }
}

/// Marker type indicating read-only access to an I/O port.
///
/// Used with `Port<T, ReadOnly>` to enforce at compile-time that
/// only read operations are possible.
pub struct ReadOnly;

/// Marker type indicating write-only access to an I/O port.
///
/// Used with `Port<T, WriteOnly>` to enforce at compile-time that
/// only write operations are possible.
pub struct WriteOnly;

/// Marker type indicating read-write access to an I/O port.
///
/// Used with `Port<T, ReadWrite>` to enforce at compile-time that
/// both read and write operations are possible.
pub struct ReadWrite;

/// A typed I/O port with access control enforced by the type system.
///
/// This generic type wraps an I/O port number and encodes access permissions
/// (read-only, write-only, or read-write) as a type parameter. This allows
/// the compiler to enforce correct access patterns at compile time.
///
/// The `Mode` type parameter determines which operations are available:
/// - `ReadOnly`: Only `read()` is available
/// - `WriteOnly`: Only `write()` is available  
/// - `ReadWrite`: Both `read()` and `write()` are available
///
/// # Example
/// ```no_run
/// use arch::ports::{Port, ReadOnly, WriteOnly};
///
/// unsafe {
///     let status_port: Port<u8, ReadOnly> = Port::new(0x60);
///     let data_port: Port<u8, WriteOnly> = Port::new(0x64);
///     
///     let value = status_port.read();
///     data_port.write(42);
/// }
/// ```
#[repr(transparent)]
pub struct Port<T: PortIO, Mode> {
    port: u16,
    _phantom: PhantomData<(T, Mode)>,
}

impl<T: PortIO, Mode> Port<T, Mode> {
    /// Creates a new `Port` for the given port number.
    ///
    /// # Arguments
    /// * `port` - The I/O port number to access
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - The port number is valid for the hardware
    /// - The access mode (read/write) is appropriate for the port
    /// - Accessing this port will not cause undefined behavior
    pub unsafe fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

impl<T: PortIO> Port<T, ReadOnly> {
    /// Reads a value from this read-only port.
    ///
    /// # Safety
    /// The caller must ensure that reading from this port is safe.
    /// The port must have been properly initialized before reading.
    pub unsafe fn read(&self) -> T {
        unsafe { T::read(self.port) }
    }
}

impl<T: PortIO> Port<T, ReadWrite> {
    /// Reads a value from this read-write port.
    ///
    /// # Safety
    /// The caller must ensure that reading from this port is safe.
    /// The port must have been properly initialized before reading.
    #[allow(unused)]
    pub unsafe fn read(&self) -> T {
        unsafe { T::read(self.port) }
    }
}

impl<T: PortIO> Port<T, WriteOnly> {
    /// Writes a value to this write-only port.
    ///
    /// # Safety
    /// The caller must ensure that writing to this port is safe.
    /// The port must have been properly initialized before writing.
    pub unsafe fn write(&self, value: T) {
        unsafe { T::write(self.port, value) }
    }
}

impl<T: PortIO> Port<T, ReadWrite> {
    /// Writes a value to this read-write port.
    ///
    /// # Safety
    /// The caller must ensure that writing to this port is safe.
    /// The port must have been properly initialized before writing.
    #[allow(unused)]
    pub unsafe fn write(&self, value: T) {
        unsafe { T::write(self.port, value) }
    }
}

/// Type aliases for common port types.
///
/// These convenience aliases provide shorter names for the most common
/// port types:
/// - `ReadOnlyPort<T>`: Read-only port of type T
/// - `WriteOnlyPort<T>`: Write-only port of type T
/// - `ReadWritePort<T>`: Read-write port of type T
pub type ReadOnlyPort<T> = Port<T, ReadOnly>;
pub type WriteOnlyPort<T> = Port<T, WriteOnly>;
#[allow(unused)]
pub type ReadWritePort<T> = Port<T, ReadWrite>;
