use core::marker::PhantomData;

/// Low-level I/O ops
#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {core::arch::asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );}
    value
}
#[inline(always)]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {core::arch::asm!(
        "in ax, dx",
        out("ax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );}
    value
}
#[inline(always)]
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {core::arch::asm!(
        "in eax, dx",
        out("eax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );}
    value
}
#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    unsafe {core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );}
}
#[inline(always)]
unsafe fn outw(port: u16, value: u16) {
    unsafe {core::arch::asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value,
        options(nomem, nostack, preserves_flags)
    );}
}
#[inline(always)]
unsafe fn outl(port: u16, value: u32) {
    unsafe {core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags)
    );}
}

/// PortIO trait

pub trait PortIO: Sized {
    unsafe fn read(port: u16) -> Self;
    unsafe fn write(port: u16, value: Self);
}

impl PortIO for u8 {
    unsafe fn read(port: u16) -> Self { unsafe{ inb(port) } }
    unsafe fn write(port: u16, value: Self) { unsafe{ outb(port, value) } }
}

impl PortIO for u16 {
    unsafe fn read(port: u16) -> Self { unsafe{ inw(port) } } 
    unsafe fn write(port: u16, value: Self) { unsafe{ outw(port, value) } }
}

impl PortIO for u32 {
    unsafe fn read(port: u16) -> Self { unsafe{ inl(port) } }
    unsafe fn write(port: u16, value: Self) { unsafe{ outl(port, value) } }
}

/// Capability markers (zero-sized types)

pub struct ReadOnly;
pub struct WriteOnly;
pub struct ReadWrite;

/// Unified Port type

pub struct Port<T: PortIO, Mode> {
    port: u16,
    _phantom: PhantomData<(T, Mode)>,
}

impl<T: PortIO, Mode> Port<T, Mode> {
    /// # Safety
    /// Caller must ensure the port is valid.
    pub unsafe fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// Read capability

impl<T: PortIO> Port<T, ReadOnly> {
    pub unsafe fn read(&self) -> T {
        unsafe {T::read(self.port) }
    }
}

impl<T: PortIO> Port<T, ReadWrite> {
    pub unsafe fn read(&self) -> T {
        unsafe { T::read(self.port) }
    }
}

/// Write capability

impl<T: PortIO> Port<T, WriteOnly> {
    pub unsafe fn write(&self, value: T) {
        unsafe { T::write(self.port, value) }
    }
}

impl<T: PortIO> Port<T, ReadWrite> {
    pub unsafe fn write(&self, value: T) {
        unsafe { T::write(self.port, value) }
    }
}

/// Type aliases for sanity
pub type ReadOnlyPort<T> = Port<T, ReadOnly>;
pub type WriteOnlyPort<T> = Port<T, WriteOnly>;
pub type ReadWritePort<T> = Port<T, ReadWrite>;