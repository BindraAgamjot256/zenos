use alloc::{string::String, vec::Vec};
use core::{
    ffi::{CStr, c_void},
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use device_prelude::bus::{Bus, BusError, Device, DeviceId};
use kprimitives::alloc::{CreatableKernelObject, GlobalAllocator, KernelObject, boxed::KBox};

use uacpi_sys::{self, uacpi_id_string, uacpi_namespace_node, uacpi_namespace_node_info};

// ============================================================
// ACPI bus
// ============================================================

#[derive(Debug)]
pub struct AcpiBus;

impl KernelObject for AcpiBus {}

impl CreatableKernelObject for AcpiBus {
    type Allocator = GlobalAllocator;
}

// ============================================================
// ACPI device
// ============================================================

#[derive(Debug)]
pub struct AcpiDevice {
    refcount: AtomicUsize,

    // Stored as an integer to avoid exposing an unguarded
    // raw pointer as a Send/Sync field.
    //
    // This does NOT independently guarantee thread safety.
    node: usize,

    // Owned hardware and compatible IDs.
    ids: Vec<DeviceId>,

    // Four-character ACPI namespace name.
    acpi_name: [u8; 4],
}

impl AcpiDevice {
    fn new(node: *mut uacpi_namespace_node, acpi_name: [u8; 4], ids: Vec<DeviceId>) -> Self {
        Self {
            refcount: AtomicUsize::new(1),
            node: node as usize,
            ids,
            acpi_name,
        }
    }

    pub fn namespace_node(&self) -> *mut uacpi_namespace_node {
        self.node as *mut uacpi_namespace_node
    }

    pub fn acpi_name(&self) -> &[u8; 4] {
        &self.acpi_name
    }
}

impl KernelObject for AcpiDevice {}

impl CreatableKernelObject for AcpiDevice {
    type Allocator = GlobalAllocator;
}

unsafe impl kprimitives::alloc::RefCounted for AcpiDevice {
    fn ref_count(&self) -> usize {
        self.refcount.load(Ordering::Relaxed)
    }

    fn inc_refcount(&self, ordering: Ordering) -> usize {
        self.refcount.fetch_add(1, ordering) + 1
    }

    fn dec_refcount(&self, ordering: Ordering) -> usize {
        self.refcount.fetch_sub(1, ordering) - 1
    }
}

impl Device for AcpiDevice {
    fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.acpi_name) }
    }

    fn ids(&self) -> &[DeviceId] {
        &self.ids
    }

    fn as_bus(&self) -> Option<KBox<dyn Bus, GlobalAllocator>> {
        // PCI bridge support belongs here eventually.
        None
    }
}

// ============================================================
// ACPI ID extraction
// ============================================================

/// Copies a uACPI string into an owned Rust String.
///
/// The returned string remains valid after the
/// namespace info allocation is freed.
///
/// SAFETY:
/// The ID must belong to a live uACPI namespace-info
/// allocation, and its value must be a valid C string.
unsafe fn copy_acpi_id(id: &uacpi_id_string) -> Option<String> {
    if id.size == 0 || id.value.is_null() {
        return None;
    }

    let value = unsafe { CStr::from_ptr(id.value.cast()) };

    match value.to_str() {
        Ok(value) if !value.is_empty() => Some(String::from(value)),
        _ => None,
    }
}

/// Extracts _HID and all _CID values.
///
/// SAFETY:
/// info must point to a valid namespace-info structure
/// returned by uacpi_get_namespace_node_info.
unsafe fn collect_device_ids(info: &uacpi_namespace_node_info) -> Vec<DeviceId> {
    let mut ids = Vec::new();

    // Primary hardware ID.
    if let Some(hid) = unsafe { copy_acpi_id(&info.hid) } {
        ids.push(DeviceId::AcpiId(hid));
    }

    // Compatible device IDs.
    let count = info.cid.num_ids as usize;

    if count > 0 {
        let cid_ptr = info.cid.ids.as_ptr();

        for index in 0..count {
            let cid = unsafe { &*cid_ptr.add(index) };

            if let Some(id) = unsafe { copy_acpi_id(cid) } {
                // Avoid duplicate IDs.
                if !ids.iter().any(|existing| {
                    matches!(
                        existing,
                        DeviceId::AcpiId(existing_id)
                            if existing_id == &id
                    )
                }) {
                    ids.push(DeviceId::AcpiId(id));
                }
            }
        }
    }

    ids
}

// ============================================================
// Enumeration context
// ============================================================

struct EnumerationContext<'a> {
    callback: &'a mut dyn FnMut(KBox<dyn Device, GlobalAllocator>),
    failed: bool,
}

// ============================================================
// uACPI namespace callback
// ============================================================

unsafe extern "C" fn descending_callback(
    opaque: *mut c_void,
    node: *mut uacpi_namespace_node,
    _depth: u32,
) -> u32 {
    const CONTINUE: u32 = uacpi_sys::uacpi_iteration_decision_UACPI_ITERATION_DECISION_CONTINUE;

    const BREAK: u32 = uacpi_sys::uacpi_iteration_decision_UACPI_ITERATION_DECISION_BREAK;

    if opaque.is_null() {
        return BREAK;
    }

    let context = unsafe { &mut *(opaque as *mut EnumerationContext<'_>) };

    if node.is_null() {
        context.failed = true;
        return BREAK;
    }

    let mut info: *mut uacpi_namespace_node_info = ptr::null_mut();

    let status = unsafe { uacpi_sys::uacpi_get_namespace_node_info(node, &mut info) };

    if status != uacpi_sys::uacpi_status::UACPI_STATUS_OK {
        let status_str =
            unsafe { CStr::from_ptr(uacpi_sys::uacpi_status_to_string(status).cast()) };

        log::error!("ACPI enumeration: {}", status_str.to_string_lossy());

        context.failed = true;
        return BREAK;
    }

    if info.is_null() {
        context.failed = true;
        return BREAK;
    }

    // Copy the four-byte ACPI namespace name.
    let acpi_name = unsafe {
        let name_ptr = (&raw const (*info).name).cast::<u8>();

        [
            *name_ptr,
            *name_ptr.add(1),
            *name_ptr.add(2),
            *name_ptr.add(3),
        ]
    };

    // Copy IDs before releasing namespace info.
    let ids = unsafe { collect_device_ids(&*info) };

    // The information allocation is no longer needed.
    unsafe {
        uacpi_sys::uacpi_free_namespace_node_info(info);
    }

    // Nodes without IDs often belong to another bus
    // (e.g. PCI devices identified through _ADR).
    // They aren't useful for ACPI ID driver matching.
    if ids.is_empty() {
        log::trace!("Skipping ACPI node {:?}: no HID/CID", acpi_name);
        return CONTINUE;
    }

    let device = AcpiDevice::new(node, acpi_name, ids);
    log::debug!("ACPI device: {}: {:?}", device.name(), device.ids());

    let device: KBox<dyn Device, GlobalAllocator> = match KBox::new(device) {
        Ok(device) => device,
        Err(_) => {
            log::error!("Failed to allocate ACPI device");
            context.failed = true;
            return BREAK;
        }
    };

    (context.callback)(device);

    CONTINUE
}

// ============================================================
// Bus implementation
// ============================================================

impl Bus for AcpiBus {
    fn name(&self) -> &'static str {
        "ACPI Bus"
    }

    fn enumerate(
        &mut self,
        callback: &mut dyn FnMut(KBox<dyn Device, GlobalAllocator>),
    ) -> Result<(), BusError> {
        let mut context = EnumerationContext {
            callback,
            failed: false,
        };

        let status = unsafe {
            uacpi_sys::uacpi_namespace_for_each_child(
                uacpi_sys::uacpi_namespace_root(),
                Some(descending_callback),
                None,
                uacpi_sys::uacpi_object_type_bits(1 << 6),
                u32::MAX,
                (&raw mut context).cast::<c_void>(),
            )
        };

        if status != uacpi_sys::uacpi_status::UACPI_STATUS_OK || context.failed {
            return Err(BusError::EnumerationFailed);
        }

        Ok(())
    }
}

// ============================================================
// Initialization
// ============================================================

pub fn init() -> Result<KBox<AcpiBus, GlobalAllocator>, &'static str> {
    KBox::new(AcpiBus).map_err(|_| "Failed to allocate ACPI bus")
}
