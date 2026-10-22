#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::{
    cell::{OnceCell, RefCell},
    ffi::{CStr, c_void},
    ops::Deref,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};
use kcore::device_core::bus::{Bus, BusError, Device, DeviceId, DeviceResource};
use kprimitives::alloc::{CreatableKernelObject, GlobalAllocator, KernelObject, boxed::KBox};
use uacpi_sys::*;

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

    // Owned hardware and compatible IDs.
    ids: Vec<DeviceId>,

    // Four-character ACPI namespace name.
    acpi_name: [u8; 4],

    resources: OnceCell<Vec<DeviceResource>>,

    node: RefCell<Option<NonNull<uacpi_namespace_node>>>,
}

impl AcpiDevice {
    fn new(acpi_name: [u8; 4], ids: Vec<DeviceId>, node: NonNull<uacpi_namespace_node>) -> Self {
        Self {
            refcount: AtomicUsize::new(1),
            ids,
            acpi_name,
            resources: OnceCell::new(),
            node: RefCell::new(Some(node)),
        }
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

    fn resources(&self) -> &[DeviceResource] {
        self.resources
            .get_or_init(|| {
                let mut mref = self.node.borrow_mut();
                let Some(node) = mref.take() else {
                    panic!("OnceCell uninit, but node None");
                };

                let res = unsafe { create_resources(node.as_ptr()) };
                if let Ok(vec) = res {
                    vec
                } else {
                    let status = res.unwrap_err();
                    if status != uacpi_sys::uacpi_status::UACPI_STATUS_OK {
                        let status_s =
                            unsafe { CStr::from_ptr(uacpi_sys::uacpi_status_to_string(status)) }
                                .to_string_lossy();
                        log::warn!("failed to parse _CRS: {}", status_s);
                    };
                    Vec::new()
                }
            })
            .deref()
    }
}
unsafe impl Send for AcpiDevice {}

#[repr(C)]
struct ResourceContext {
    resources: Vec<DeviceResource>,
}

unsafe extern "C" fn collect_resource(
    user: *mut c_void,
    resource: *mut uacpi_resource,
) -> uacpi_iteration_decision {
    if user.is_null() || resource.is_null() {
        return uacpi_iteration_decision_UACPI_ITERATION_DECISION_CONTINUE;
    }

    // SAFETY: `user` points to the live ResourceContext passed to
    // uacpi_for_each_device_resource. `resource` is valid for this callback.
    let context = unsafe { &mut *user.cast::<ResourceContext>() };
    let resource = unsafe { &*resource };

    let resource_type = uacpi_resource_type(resource.type_);

    match resource_type {
        uacpi_resource_type::UACPI_RESOURCE_TYPE_IRQ => {
            let irq = unsafe { resource.__bindgen_anon_1.irq.as_ref() };
            let numbers = irq.irqs.as_ptr();

            for i in 0..irq.num_irqs as usize {
                let number = unsafe { ptr::read(numbers.add(i)) } as u32;

                context.resources.push(DeviceResource::IRQ {
                    number,
                    triggering: irq.triggering,
                    polarity: irq.polarity,
                    sharing: irq.sharing,
                    wake_capability: irq.wake_capability,
                });
            }
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_EXTENDED_IRQ => {
            let irq = unsafe { resource.__bindgen_anon_1.extended_irq.as_ref() };
            let numbers = irq.irqs.as_ptr();

            for i in 0..irq.num_irqs as usize {
                let number = unsafe { ptr::read(numbers.add(i)) };

                context.resources.push(DeviceResource::ExtendedIRQ {
                    number,
                    triggering: irq.triggering,
                    polarity: irq.polarity,
                    sharing: irq.sharing,
                    wake_capability: irq.wake_capability,
                });
            }
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_IO => {
            let io = unsafe { resource.__bindgen_anon_1.io.as_ref() };

            context.resources.push(DeviceResource::IoPort {
                base: io.minimum as usize,
                len: io.length as usize,
            });
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_FIXED_IO => {
            let io = unsafe { resource.__bindgen_anon_1.fixed_io.as_ref() };

            context.resources.push(DeviceResource::IoPort {
                base: io.address as usize,
                len: io.length as usize,
            });
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_MEMORY24 => {
            let mem = unsafe { resource.__bindgen_anon_1.memory24.as_ref() };

            context.resources.push(DeviceResource::MMIO {
                base: mem.minimum as usize,
                len: mem.length as usize,
            });
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_MEMORY32 => {
            let mem = unsafe { resource.__bindgen_anon_1.memory32.as_ref() };

            context.resources.push(DeviceResource::MMIO {
                base: mem.minimum as usize,
                len: mem.length as usize,
            });
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_FIXED_MEMORY32 => {
            let mem = unsafe { resource.__bindgen_anon_1.fixed_memory32.as_ref() };

            context.resources.push(DeviceResource::MMIO {
                base: mem.address as usize,
                len: mem.length as usize,
            });
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_ADDRESS16 => {
            let r = unsafe { resource.__bindgen_anon_1.address16.as_ref() };

            push_address_resource(
                &mut context.resources,
                r.common.type_ as uacpi_address_space,
                r.minimum as u64,
                r.address_length as u64,
            );
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_ADDRESS32 => {
            let r = unsafe { resource.__bindgen_anon_1.address32.as_ref() };

            push_address_resource(
                &mut context.resources,
                r.common.type_ as uacpi_address_space,
                r.minimum as u64,
                r.address_length as u64,
            );
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_ADDRESS64 => {
            let r = unsafe { resource.__bindgen_anon_1.address64.as_ref() };

            push_address_resource(
                &mut context.resources,
                r.common.type_ as uacpi_address_space,
                r.minimum,
                r.address_length,
            );
        }

        uacpi_resource_type::UACPI_RESOURCE_TYPE_ADDRESS64_EXTENDED => {
            let r = unsafe { resource.__bindgen_anon_1.address64_extended.as_ref() };

            push_address_resource(
                &mut context.resources,
                r.common.type_ as uacpi_address_space,
                r.minimum,
                r.address_length,
            );
        }

        // Unsupported descriptors are intentionally ignored.
        _ => {}
    }

    uacpi_iteration_decision_UACPI_ITERATION_DECISION_CONTINUE
}

fn push_address_resource(
    resources: &mut Vec<DeviceResource>,
    address_space: uacpi_address_space,
    base: u64,
    len: u64,
) {
    if len == 0 {
        return;
    }

    match address_space {
        uacpi_sys::uacpi_address_space_UACPI_ADDRESS_SPACE_SYSTEM_MEMORY => {
            resources.push(DeviceResource::MMIO {
                base: base as usize,
                len: len as usize,
            });
        }

        uacpi_sys::uacpi_address_space_UACPI_ADDRESS_SPACE_SYSTEM_IO => {
            resources.push(DeviceResource::IoPort {
                base: base as usize,
                len: len as usize,
            });
        }

        _ => {}
    }
}

unsafe fn create_resources(
    node: *mut uacpi_namespace_node,
) -> Result<Vec<DeviceResource>, uacpi_status> {
    if node.is_null() {
        return Err(uacpi_status::UACPI_STATUS_INVALID_ARGUMENT);
    }

    let mut context = ResourceContext {
        resources: Vec::new(),
    };

    let status = unsafe {
        uacpi_for_each_device_resource(
            node,
            c"_CRS".as_ptr(),
            Some(collect_resource),
            ptr::addr_of_mut!(context).cast::<c_void>(),
        )
    };

    if status != uacpi_status::UACPI_STATUS_OK {
        return Err(status);
    }

    Ok(context.resources)
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

struct EnumerationContext<'a> {
    callback: &'a mut dyn FnMut(KBox<dyn Device, GlobalAllocator>),
    failed: bool,
}

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

    let mut status_flags: u32 = 0;
    let status = unsafe { uacpi_sys::uacpi_eval_sta(node, &mut status_flags) };

    if status == uacpi_sys::uacpi_status::UACPI_STATUS_OK {
        // Bit 0 (0x01) means "Device is present"
        // Bit 1 (0x02) means "Device is enabled and decoding its resources"
        let present_and_enabled = 0x03;
        if (status_flags & present_and_enabled) != present_and_enabled {
            log::info!(
                "Skipping inactive ACPI node {:?}",
                core::str::from_utf8(&acpi_name)
            );
            return CONTINUE;
        }
    }

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
        log::info!(
            "Skipping ACPI node {:?}: no HID/CID",
            core::str::from_utf8(&acpi_name)
        );
        return CONTINUE;
    }

    let device = AcpiDevice::new(acpi_name, ids, unsafe { NonNull::new_unchecked(node) });
    log::debug!("ACPI device: {}({:?})", device.name(), device.ids());

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
