use crate::memory::{PAGE_4K, PageType, kalloc_page};
use acpi::PhysicalMapping;
use core::ptr::NonNull;
use log::debug;
use x86_64::VirtAddr;

#[derive(Copy, Clone, Debug)]
pub struct AcpiHandler;

impl acpi::AcpiHandler for AcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let mut virt =
            VirtAddr::new(physical_address as u64 + crate::memory::constants::HIGHER_HALF_BASE);

        let times = size / PAGE_4K;
        for _ in 0..times {
            let result = kalloc_page(virt, PageType::Recursive);
            if let Err(e) = result {
                log::warn!("Failed to allocate a page for ACPI: {e:?}");
            }
            virt = VirtAddr::new(virt.as_u64() + PAGE_4K as u64);
        }

        let map = PhysicalMapping::new(
            physical_address,
            NonNull::new_unchecked(virt.as_u64() as *mut T),
            size,
            ((size / PAGE_4K) + 1) * PAGE_4K,
            Self,
        );
        debug!("created mapping: {map:#?}");
        map
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
        // APIC tables are tiny static regions of memory. at the max, we only require 40kib, even if
        // each table is present in a separate page. Keeping this into account, we can ignore unmapping.

        // tldr: I was too lazy to implement this
        x86_64::instructions::nop();
    }
}
