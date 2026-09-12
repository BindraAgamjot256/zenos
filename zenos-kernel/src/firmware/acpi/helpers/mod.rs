#![expect(unused)]
use crate::{firmware::acpi::helpers::mutex::UacpiMutex, mm::GlobalAllocator};
use core::{
    alloc::Layout,
    arch, mem,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};
use kprimitives::alloc::{Allocation, Allocator, boxed::KBox};

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

    let ptr: *mut () = Layout::from_size_align(size as usize, 1)
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
        log::warn!("uacpi_kernel_free: null pointer");
        return;
    }

    let layout = match Layout::from_size_align(size as usize, 1) {
        Ok(layout) => layout,
        Err(_) => {
            log::error!("uacpi_kernel_free: invalid layout, size={:#x}", size);
            return;
        }
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
        uacpi_sys::uacpi_log_level::UACPI_LOG_DEBUG => log::debug!("{}", msg),
        uacpi_sys::uacpi_log_level::UACPI_LOG_TRACE => log::trace!("{}", msg),
        uacpi_sys::uacpi_log_level::UACPI_LOG_INFO => log::info!("{}", msg),
        uacpi_sys::uacpi_log_level::UACPI_LOG_WARN => log::warn!("{}", msg),
        uacpi_sys::uacpi_log_level::UACPI_LOG_ERROR => log::error!("{}", msg),
        _ => log::debug!("(unknown level) {}", msg),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_mutex() -> uacpi_sys::uacpi_handle {
    let handle: uacpi_sys::uacpi_handle = KBox::new(mutex::UacpiMutex::new())
        .ok()
        .and_then(|mutex| Some(mutex.raw_ptr().cast()))
        .unwrap_or(core::ptr::null_mut());

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
    unsafe { KBox::from_raw(handle.cast::<UacpiMutex>(), mutex::Allocator) };
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_spinlock() -> uacpi_sys::uacpi_handle {
    uacpi_kernel_create_mutex()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free_spinlock(handle: uacpi_sys::uacpi_handle) {
    uacpi_kernel_free_mutex(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_lock_spinlock(
    handle: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_cpu_flags {
    log::trace!("uacpi_kernel_lock_spinlock: handle={:p}", handle);
    let kbox = unsafe { KBox::from_raw(handle.cast::<UacpiMutex>(), mutex::Allocator) };
    kbox.lock();
    core::mem::forget(kbox);
    log::trace!("uacpi_kernel_lock_spinlock: acquired handle={:p}", handle);
    0
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
    let kbox = unsafe { KBox::from_raw(handle.cast::<UacpiMutex>(), mutex::Allocator) };
    kbox.unlock();
    core::mem::forget(kbox);
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
        let kbox =
            KBox::new(self::io::IoPort::new(base, len as u64)).and_then(|b| Ok(b.raw_ptr().cast()));

        if kbox.is_err() {
            log::error!(
                "uacpi_kernel_io_map: failed to allocate IO handle, base={:#x}, len={:#x}",
                base,
                len
            );
            return todo!();
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
    log::debug!("uacpi_kernel_get_thread_id: not implemented, returning null");
    core::ptr::null_mut() // still todo.
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

    let kbox = unsafe { KBox::from_raw(handle.cast::<UacpiMutex>(), mutex::Allocator) };

    let mut time = 0;

    loop {
        if kbox.try_lock() {
            break;
        }
        if time >= timeout {
            log::warn!(
                "uacpi_kernel_acquire_mutex: timeout handle={:p}, waited={}",
                handle,
                time
            );
            return uacpi_sys::uacpi_status::UACPI_STATUS_TIMEOUT;
        }
        time += 1;
    }

    mem::forget(kbox);
    log::trace!("uacpi_kernel_acquire_mutex: acquired handle={:p}", handle);
    uacpi_sys::uacpi_status::UACPI_STATUS_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_release_mutex(handle: uacpi_sys::uacpi_handle) {
    log::trace!("uacpi_kernel_release_mutex: handle={:p}", handle);
    uacpi_kernel_unlock_spinlock(handle, 0);
}

// ===================================
// TODOS:
// ===================================

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_get_nanoseconds_since_boot() -> uacpi_sys::uacpi_u64 {
    log::error!("uacpi_kernel_get_nanoseconds_since_boot: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_device_open(
    address: uacpi_sys::uacpi_pci_address,
    out_handle: *mut uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_device_open: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_device_close(handle: uacpi_sys::uacpi_handle) {
    log::error!("uacpi_kernel_pci_device_close: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_read8: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_read16: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_read32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    out_value: *mut uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_read32: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write8(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u8,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_write8: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write16(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u16,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_write16: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_pci_write32(
    handle: uacpi_sys::uacpi_handle,
    offset: uacpi_sys::uacpi_size,
    value: uacpi_sys::uacpi_u32,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_pci_write32: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_stall(usec: uacpi_sys::uacpi_u8) {
    log::error!("uacpi_kernel_stall: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_sleep(msec: uacpi_sys::uacpi_u64) {
    log::error!("uacpi_kernel_sleep: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_create_event() -> uacpi_sys::uacpi_handle {
    log::error!("uacpi_kernel_create_event: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_free_event(handle: uacpi_sys::uacpi_handle) {
    log::error!("uacpi_kernel_free_event: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_disable_interrupts() -> uacpi_sys::uacpi_interrupt_state {
    log::error!("uacpi_kernel_disable_interrupts: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_restore_interrupts(state: uacpi_sys::uacpi_interrupt_state) {
    log::error!("uacpi_kernel_restore_interrupts: NOT IMPLEMENTED");
    todo!()
}

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
pub extern "C" fn uacpi_kernel_install_interrupt_handler(
    irq: uacpi_sys::uacpi_u32,
    handler: uacpi_sys::uacpi_interrupt_handler,
    ctx: uacpi_sys::uacpi_handle,
    out_handle: *mut uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_install_interrupt_handler: NOT IMPLEMENTED");
    todo!()
}

#[unsafe(no_mangle)]
pub extern "C" fn uacpi_kernel_uninstall_interrupt_handler(
    handler: uacpi_sys::uacpi_interrupt_handler,
    irq_handle: uacpi_sys::uacpi_handle,
) -> uacpi_sys::uacpi_status {
    log::error!("uacpi_kernel_uninstall_interrupt_handler: NOT IMPLEMENTED");
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
