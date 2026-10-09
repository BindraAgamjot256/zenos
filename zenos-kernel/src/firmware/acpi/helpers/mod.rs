#![expect(unused)]
use crate::{
    arch::{CpuContext, common::SpinLock, disable_interrupts_and_save, restore_interrupts},
    firmware::acpi::helpers::mutex::{UacpiMutex, UacpiSpinlock},
    irq::{IRQ_CONTROLLER, IrqGuard},
    mm::GlobalAllocator,
    scheduler::SCHEDULER,
};
use alloc::{boxed::Box, collections::BTreeMap};
use core::{
    alloc::Layout,
    arch, mem,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use kprimitives::alloc::{Allocation, Allocator, CreatableKernelObject, KernelObject, boxed::KBox};
use scheduler::Thread;

mod events;
mod io;
mod mutex;

static RSDP_ADDR: AtomicUsize = AtomicUsize::new(0);

pub fn init(rsdp_addr: usize) {
    log::info!("uACPI kernel interface initialized: RSDP={:#x}", rsdp_addr);
    RSDP_ADDR.store(rsdp_addr, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_get_rsdp(
    out_rsdp_address: *mut uacpi_sys::uacpi_phys_addr,
) -> uacpi_sys::uacpi_status {
    log::trace!("uacpi_kernel_get_rsdp(out={:p})", out_rsdp_address);

    if out_rsdp_address.is_null() {
        log::warn!("uacpi_kernel_get_rsdp: null output pointer");
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let rsdp_addr = RSDP_ADDR.load(Ordering::Relaxed);

    if rsdp_addr == 0 {
        log::warn!("uacpi_kernel_get_rsdp: RSDP address is not initialized");
        return uacpi_sys::uacpi_status::UACPI_STATUS_NOT_FOUND;
    }

    unsafe {
        *out_rsdp_address = rsdp_addr as uacpi_sys::uacpi_phys_addr;
    }

    log::trace!("uacpi_kernel_get_rsdp: returning {:#x}", rsdp_addr);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_map(
    addr: uacpi_sys::uacpi_phys_addr,
    len: uacpi_sys::uacpi_size,
) -> *mut core::ffi::c_void {
    let offset = crate::arch::mem::get_phys_offset();
    let virt_addr = (addr as usize) + offset;

    log::trace!(
        "uacpi_kernel_map: phys={:#x}, len={:#x}, virt={:#x}, offset={:#x}",
        addr,
        len,
        virt_addr,
        offset
    );

    core::ptr::with_exposed_provenance_mut(virt_addr)
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_unmap(ptr: *mut core::ffi::c_void, len: uacpi_sys::uacpi_size) {
    log::trace!(
        "uacpi_kernel_unmap: ptr={:p}, len={:#x} (HHDM no-op)",
        ptr,
        len
    );
    // Do nothing, since HHDM is always mapped.
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_alloc(size: uacpi_sys::uacpi_size) -> *mut core::ffi::c_void {
    log::trace!("uacpi_kernel_alloc: size={:#x}", size);

    let ptr: *mut () = Layout::from_size_align(size, size.next_power_of_two())
        .ok()
        .and_then(|layout| crate::mm::GlobalAllocator::alloc_zeroed(layout).ok())
        .map(|ptr| ptr.as_ptr().cast().as_ptr())
        .unwrap_or(core::ptr::null_mut());

    if ptr.is_null() {
        log::error!("uacpi_kernel_alloc: FAILED, size={:#x}", size);
    } else {
        log::trace!("uacpi_kernel_alloc: ptr={:p}, size={:#x}", ptr, size);
    }

    ptr.cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free(ptr: *mut core::ffi::c_void, size: uacpi_sys::uacpi_size) {
    log::trace!("uacpi_kernel_free: ptr={:p}, size={:#x}", ptr, size);

    if ptr.is_null() {
        return;
    }

    let Ok(layout) = Layout::from_size_align(size, size.next_power_of_two()) else {
        return;
    };

    unsafe {
        crate::mm::GlobalAllocator::deallocate(
            Allocation::from_ptr(NonNull::new_unchecked(ptr.cast())),
            layout,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_log(
    level: uacpi_sys::uacpi_log_level,
    message: *const uacpi_sys::uacpi_char,
) {
    if message.is_null() {
        return;
    }

    let msg = unsafe { core::ffi::CStr::from_ptr(message) }.to_string_lossy();
    match level {
        uacpi_sys::uacpi_log_level::UACPI_LOG_DEBUG => {
            log::trace!(target: "uacpi", "{}", msg)
        }
        uacpi_sys::uacpi_log_level::UACPI_LOG_TRACE => {
            log::debug!(target: "uacpi", "{}", msg)
        }
        uacpi_sys::uacpi_log_level::UACPI_LOG_INFO => {
            log::info!(target: "uacpi", "{}", msg)
        }
        uacpi_sys::uacpi_log_level::UACPI_LOG_WARN => {
            log::warn!(target: "uacpi", "{}", msg)
        }
        uacpi_sys::uacpi_log_level::UACPI_LOG_ERROR => {
            log::error!(target: "uacpi", "{}", msg)
        }
        _ => {}
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_spinlock() -> uacpi_sys::uacpi_handle {
    KBox::new(UacpiSpinlock::new())
        .map(|spinlock| spinlock.raw_ptr().cast())
        .unwrap_or(core::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free_spinlock(handle: uacpi_sys::uacpi_handle) {
    log::trace!("uacpi_kernel_free_spinlock: handle={:p}", handle);

    if handle.is_null() {
        log::warn!("uacpi_kernel_free_spinlock: null handle");
        return;
    }

    drop(unsafe { KBox::from_raw(handle.cast::<UacpiSpinlock>(), GlobalAllocator) });
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_lock_spinlock(
    handle: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_cpu_flags {
    log::trace!("uacpi_kernel_lock_spinlock: handle={:p}", handle);

    let flags = unsafe { &*handle.cast::<UacpiSpinlock>() }.lock();
    log::trace!("uacpi_kernel_lock_spinlock: acquired handle={:p}", handle);

    flags as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_unlock_spinlock(
    handle: uacpi_sys::uacpi_handle,
    flags: uacpi_sys::uacpi_cpu_flags,
) {
    log::trace!(
        "uacpi_kernel_unlock_spinlock: handle={:p}, flags={:#x}",
        handle,
        flags
    );

    if !handle.is_null() {
        unsafe { &*handle.cast::<UacpiSpinlock>() }.unlock(flags as usize);
    }
    log::trace!("uacpi_kernel_unlock_spinlock: released handle={:p}", handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_map(
    base: uacpi_sys::uacpi_io_addr,
    len: uacpi_sys::uacpi_size,
    out_handle: *mut uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_map: base={:#x}, len={:#x}, out_handle={:p}",
        base,
        len,
        out_handle
    );

    unsafe {
        let kbox = KBox::new(self::io::IoPort::new(base, len as u64)).map(|b| b.raw_ptr().cast());

        if kbox.is_err() {
            log::error!(
                "uacpi_kernel_io_map: failed to allocate IO handle, base={:#x}, len={:#x}",
                base,
                len
            );
            return uacpi_sys::uacpi_status::UACPI_STATUS_OUT_OF_MEMORY;
        }
        *out_handle = kbox.unwrap();
        log::trace!("uacpi_kernel_io_map: handle={:p}", *out_handle);
    }
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_unmap(handle: uacpi_sys::uacpi_handle) {
    log::trace!("uacpi_kernel_io_unmap: handle={:p}", handle);
    let kbox = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };

    drop(kbox);
    log::trace!("uacpi_kernel_io_unmap: released handle={:p}", handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_read8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_read8: handle={:p}, offset={:#x}, out={:p}",
        handle,
        offset,
        out_value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::ReadOnlyPort::new(port_base) };
    let value = unsafe { port.read() };
    unsafe { *out_value = value };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_read8: port={:#x} -> value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_read16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_read16: handle={:p}, offset={:#x}, out={:p}",
        handle,
        offset,
        out_value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::ReadOnlyPort::new(port_base) };
    let value = unsafe { port.read() };
    unsafe { *out_value = value };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_read16: port={:#x} -> value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_read32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_read32: handle={:p}, offset={:#x}, out={:p}",
        handle,
        offset,
        out_value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::ReadOnlyPort::new(port_base) };
    let value = unsafe { port.read() };
    unsafe { *out_value = value };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_read32: port={:#x} -> value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_write8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_write8: handle={:p}, offset={:#x}, value={:#x}",
        handle,
        offset,
        value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::WriteOnlyPort::new(port_base) };
    unsafe { port.write(value) };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_write8: port={:#x} <- value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_write16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_write16: handle={:p}, offset={:#x}, value={:#x}",
        handle,
        offset,
        value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::WriteOnlyPort::new(port_base) };
    unsafe { port.write(value) };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_write16: port={:#x} <- value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_io_write32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_io_write32: handle={:p}, offset={:#x}, value={:#x}",
        handle,
        offset,
        value
    );
    let io_port = unsafe { KBox::from_raw(handle as *mut self::io::IoPort, GlobalAllocator) };
    let port_base = (io_port.base + offset as u64) as u16;
    let port = unsafe { crate::arch::ports::WriteOnlyPort::new(port_base) };
    unsafe { port.write(value) };
    mem::forget(io_port);
    log::trace!(
        "uacpi_kernel_io_write32: port={:#x} <- value={:#x}",
        port_base,
        value
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_get_thread_id() -> uacpi_sys::uacpi_thread_id {
    crate::scheduler::current_thread()
        .map(|c| c.tidptr())
        .unwrap_or_default()
        .cast_mut()
        .cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_mutex() -> uacpi_sys::uacpi_handle {
    let handle: uacpi_sys::uacpi_handle = KBox::new(mutex::UacpiMutex::new())
        .ok()
        .map(|mutex| mutex.raw_ptr().cast())
        .unwrap_or_default();

    if handle.is_null() {
        log::error!("uacpi_kernel_create_mutex: FAILED");
    } else {
        log::trace!("uacpi_kernel_create_mutex: handle={:p}", handle);
    }

    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free_mutex(handle: uacpi_sys::uacpi_handle) {
    log::trace!("uacpi_kernel_free_mutex: handle={:p}", handle);

    if handle.is_null() {
        log::warn!("uacpi_kernel_free_mutex: null handle");
        return;
    }

    drop(unsafe { KBox::from_raw(handle.cast::<UacpiMutex>(), GlobalAllocator) });
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_acquire_mutex(
    handle: uacpi_sys::uacpi_handle,
    timeout: uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_acquire_mutex: handle={:p}, timeout={}",
        handle,
        timeout
    );

    if handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let mutex = unsafe { &*handle.cast::<UacpiMutex>() };
    let mut attempts = 0;

    loop {
        // TODO: sleep until a monotonic deadline instead of polling.
        if mutex.try_lock() {
            log::trace!("uacpi_kernel_acquire_mutex: acquired handle={:p}", handle);
            return uacpi_sys::uacpi_status::UACPI_STATUS_OK;
        }
        if timeout != u16::MAX && attempts >= timeout {
            log::warn!(
                "uacpi_kernel_acquire_mutex: timeout handle={:p}, attempts={}",
                handle,
                attempts
            );
            return uacpi_sys::uacpi_status::UACPI_STATUS_TIMEOUT;
        }
        attempts = attempts.saturating_add(1);
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_release_mutex(handle: uacpi_sys::uacpi_handle) {
    log::trace!("uacpi_kernel_release_mutex: handle={:p}", handle);

    if handle.is_null() {
        log::warn!("uacpi_kernel_release_mutex: null handle");
        return;
    }

    unsafe { &*handle.cast::<UacpiMutex>() }.unlock();
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_get_nanoseconds_since_boot() -> uacpi_sys::uacpi_u64 {
    let clocksource = crate::arch::CLOCKSOURCE.read();
    if let Some(clock) = clocksource.as_ref() {
        clock.get_nanoseconds_since_boot()
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_stall(usec: uacpi_sys::uacpi_u8) {
    let clocksource = crate::arch::CLOCKSOURCE.read();
    let Some(clock) = clocksource.as_ref() else {
        return;
    };
    let duration = core::time::Duration::from_micros(usec as u64);
    let start = clock.now();
    while clock.delta_now(start) < duration {
        core::hint::spin_loop();
    }
}

struct UacpiInterrupt(uacpi_sys::uacpi_interrupt_handler, uacpi_sys::uacpi_handle);

// safe since the uacpi_handle that causes !send et all is gonna exist for longer than the static will stay
// in the btree map(guaranteed by the uacpi implementation)
unsafe impl Send for UacpiInterrupt {}
unsafe impl Sync for UacpiInterrupt {}

static IRQ_HANDLERS: SpinLock<BTreeMap<u8, UacpiInterrupt>> = SpinLock::new(BTreeMap::new());

fn generic_uacpi_handler(ctx: &mut CpuContext) {
    let handlers = IRQ_HANDLERS.lock();
    let Some(handler) = handlers.get(&(ctx.vector() as u8)) else {
        return;
    };
    let Some(handlerfn) = handler.0 else {
        return;
    };
    unsafe { handlerfn(handler.1) };
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_install_interrupt_handler(
    irq: uacpi_sys::uacpi_u32,
    handler: uacpi_sys::uacpi_interrupt_handler,
    ctx: uacpi_sys::uacpi_handle,
    out_handle: *mut uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::info!("uacpi_kernel_install_interrupt_handler");
    let cont = IRQ_CONTROLLER.read();
    let Some(mut handle) = cont.set_number(irq) else {
        log::error!("Failed to set number for IRQ {}", irq);
        return uacpi_sys::uacpi_status::UACPI_STATUS_INTERNAL_ERROR;
    };

    handle.set_handler(generic_uacpi_handler);
    handle.unmask();

    let handle = Box::into_raw(Box::new(handle));
    let mut irq_handlers = IRQ_HANDLERS.lock();

    irq_handlers.insert(irq as u8, UacpiInterrupt(handler, ctx));

    log::info!(
        "Registered IRQ handler for IRQ(GSI) {}, handler: {:#x}",
        irq,
        handler.map(|h| h as usize).unwrap_or(0)
    );

    unsafe { *out_handle = handle.cast() };
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_uninstall_interrupt_handler(
    _: uacpi_sys::uacpi_interrupt_handler,
    irq_handle: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    let mut handle = unsafe { Box::from_raw(irq_handle.cast::<IrqGuard>()) };
    let irq = handle.vector();
    handle.mask();
    let _ = IRQ_HANDLERS.lock().remove(&irq);
    drop(handle);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_disable_interrupts() -> uacpi_sys::uacpi_interrupt_state {
    disable_interrupts_and_save() as uacpi_sys::uacpi_interrupt_state
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_restore_interrupts(state: uacpi_sys::uacpi_interrupt_state) {
    restore_interrupts(state as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_event() -> uacpi_sys::uacpi_handle {
    let Ok(event) = KBox::new(events::UacpiEvent::new()) else {
        return core::ptr::null_mut();
    };
    let handle = KBox::raw_ptr(event);
    handle.cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free_event(handle: uacpi_sys::uacpi_handle) {
    let tbox = unsafe { KBox::from_raw(handle.cast::<events::UacpiEvent>(), GlobalAllocator) };
    drop(tbox);
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_sleep(msec: uacpi_sys::uacpi_u64) {
    let duration = Duration::from_millis(msec);
    log::info!("Sleeping for {msec}ms");
    SCHEDULER.sleep_current(duration);
}

/// The internal tracking object allocated on your kernel heap via KBox.
/// This matches your kernel's pattern of embedding raw tracking info into the handle pointer.
// TODO: enumerate PCI, and use the enumerated ones here.
// The current impl was generated by AI.
pub struct OpaquePciDevice {
    pub address: uacpi_sys::uacpi_pci_address,
}

impl KernelObject for OpaquePciDevice {}

impl CreatableKernelObject for OpaquePciDevice {
    type Allocator = GlobalAllocator;
}

/// Helper function to construct the 32-bit configuration address value for port 0xCF8
#[inline(always)]
fn make_pci_config_address(
    address: &uacpi_sys::uacpi_pci_address,
    offset: uacpi_sys::uacpi_size,
) -> u32 {
    ((address.bus as u32) << 16)
        | ((address.device as u32) << 11)
        | ((address.function as u32) << 8)
        | (offset as u32 & 0xFC)
        | 0x8000_0000 // Configuration Enable Bit
}

// =========================================================================
// PCI DEVICE LIFECYCLE MANAGEMENT
// =========================================================================

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_device_open(
    address: uacpi_sys::uacpi_pci_address,
    out_handle: *mut uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::trace!(
        "uacpi_kernel_pci_device_open: seg={}, bus={}, dev={}, fn={}, out={:p}",
        address.segment,
        address.bus,
        address.device,
        address.function,
        out_handle
    );

    if out_handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    // Direct hardware check using your kernel's raw architecture logic.
    // We check if a valid vendor ID exists at offset 0x00 before claiming success.
    let config_addr = make_pci_config_address(&address, 0);
    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port = unsafe { crate::arch::ports::ReadOnlyPort::<u32>::new(0xCFC) };

    let vendor_device_id = unsafe {
        address_port.write(config_addr);
        data_port.read()
    };

    // 0xFFFF or 0xFFFFFFFF means the device does not exist on this bus layout
    if (vendor_device_id & 0xFFFF) == 0xFFFF {
        return uacpi_sys::uacpi_status::UACPI_STATUS_NOT_FOUND;
    }

    // Allocate tracking context using your safe kernel heap framework
    let kbox = KBox::new(OpaquePciDevice { address }).map(|b| b.raw_ptr().cast());

    if kbox.is_err() {
        log::error!("uacpi_kernel_pci_device_open: failed to allocate PCI device tracking state");
        return uacpi_sys::uacpi_status::UACPI_STATUS_OUT_OF_MEMORY;
    }

    unsafe { *out_handle = kbox.unwrap() };
    log::trace!("uacpi_kernel_pci_device_open: handle={:p}", unsafe {
        *out_handle
    });

    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_device_close(
    handle: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::trace!("uacpi_kernel_pci_device_close: handle={:p}", handle);
    if handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    // Reclaim memory allocations to prevent internal driver heap leaks
    let kbox = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    drop(kbox);

    log::trace!(
        "uacpi_kernel_pci_device_close: released handle={:p}",
        handle
    );
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

// =========================================================================
// PCI CONFIGURATION READ OPERATIONS
// =========================================================================

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() || out_value.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    // Read from the 8-bit offset location offset directly inside the 32-bit window
    let data_port =
        unsafe { crate::arch::ports::ReadOnlyPort::<u8>::new(0xCFC + (offset as u16 & 3)) };

    let value = unsafe {
        address_port.write(config_addr);
        data_port.read()
    };

    unsafe { *out_value = value };
    mem::forget(dev); // Maintain handle ownership inside the runtime heap

    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() || out_value.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port =
        unsafe { crate::arch::ports::ReadOnlyPort::<u16>::new(0xCFC + (offset as u16 & 2)) };

    let value = unsafe {
        address_port.write(config_addr);
        data_port.read()
    };

    unsafe { *out_value = value };
    mem::forget(dev);

    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() || out_value.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port = unsafe { crate::arch::ports::ReadOnlyPort::<u32>::new(0xCFC) };

    let value = unsafe {
        address_port.write(config_addr);
        data_port.read()
    };

    unsafe { *out_value = value };
    mem::forget(dev);

    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

// =========================================================================
// PCI CONFIGURATION WRITE OPERATIONS
// =========================================================================

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port =
        unsafe { crate::arch::ports::WriteOnlyPort::<u8>::new(0xCFC + (offset as u16 & 3)) };

    unsafe {
        address_port.write(config_addr);
        data_port.write(value);
    }

    mem::forget(dev);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port =
        unsafe { crate::arch::ports::WriteOnlyPort::<u16>::new(0xCFC + (offset as u16 & 2)) };

    unsafe {
        address_port.write(config_addr);
        data_port.write(value);
    }

    mem::forget(dev);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    if handle.is_null() {
        return uacpi_sys::uacpi_status::UACPI_STATUS_INVALID_ARGUMENT;
    }

    let dev = unsafe { KBox::from_raw(handle as *mut OpaquePciDevice, GlobalAllocator) };
    let config_addr = make_pci_config_address(&dev.address, offset);

    let address_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCF8) };
    let data_port = unsafe { crate::arch::ports::WriteOnlyPort::<u32>::new(0xCFC) };

    unsafe {
        address_port.write(config_addr);
        data_port.write(value);
    }

    mem::forget(dev);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

// ===================================
// TODOS:
// ===================================

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_wait_for_event(
    handle: uacpi_sys::uacpi_handle,
    timeout: uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_bool {
    log::error!("uacpi_kernel_wait_for_event: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_signal_event(handle: uacpi_sys::uacpi_handle) {
    log::error!("uacpi_kernel_signal_event: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_reset_event(handle: uacpi_sys::uacpi_handle) {
    log::error!("uacpi_kernel_reset_event: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_handle_firmware_request(
    request: *mut uacpi_sys::uacpi_firmware_request,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_handle_firmware_request: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_schedule_work(
    work_type: uacpi_sys::uacpi_work_type,
    handler: uacpi_sys::uacpi_work_handler,
    ctx: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_schedule_work: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_wait_for_work_completion() -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_wait_for_work_completion: NOT IMPLEMENTED");
    todo!()
}
