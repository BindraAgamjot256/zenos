#![allow(unused)]
//! Contains a [`Handler`] implementation for ACPI([`AcpiHandler`]), used by the ACPI subsystem.

use crate::arch::ports::ReadWritePort;
use acpi::Handler;
use core::ptr::NonNull;

#[derive(Debug, Clone, Copy)]
pub struct AcpiHandler;

impl Handler for AcpiHandler {
    #[inline(always)]
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> acpi::PhysicalMapping<Self, T> {
        let virt = crate::arch::mem::get_phys_offset() + physical_address;
        acpi::PhysicalMapping {
            physical_start: physical_address,
            virtual_start: NonNull::new(virt as *mut T).expect("virtual address must not be null"),
            region_length: size,
            mapped_length: size,
            handler: Self,
        }
    }

    #[inline(always)]
    fn unmap_physical_region<T>(region: &acpi::PhysicalMapping<Self, T>) {} // nothing to do, HHDM is permanent.

    #[inline(always)]
    fn read_u8(&self, address: usize) -> u8 {
        unsafe {
            core::ptr::read_volatile((crate::arch::mem::get_phys_offset() + address) as *const u8)
        }
    }

    #[inline(always)]
    fn read_u16(&self, address: usize) -> u16 {
        unsafe {
            core::ptr::read_volatile((crate::arch::mem::get_phys_offset() + address) as *const u16)
        }
    }

    #[inline(always)]
    fn read_u32(&self, address: usize) -> u32 {
        unsafe {
            core::ptr::read_volatile((crate::arch::mem::get_phys_offset() + address) as *const u32)
        }
    }

    #[inline(always)]
    fn read_u64(&self, address: usize) -> u64 {
        unsafe {
            core::ptr::read_volatile((crate::arch::mem::get_phys_offset() + address) as *const u64)
        }
    }

    #[inline(always)]
    fn write_u8(&self, address: usize, value: u8) {
        unsafe {
            core::ptr::write_volatile(
                (crate::arch::mem::get_phys_offset() + address) as *mut u8,
                value,
            )
        }
    }

    #[inline(always)]
    fn write_u16(&self, address: usize, value: u16) {
        unsafe {
            core::ptr::write_volatile(
                (crate::arch::mem::get_phys_offset() + address) as *mut u16,
                value,
            )
        }
    }

    #[inline(always)]
    fn write_u32(&self, address: usize, value: u32) {
        unsafe {
            core::ptr::write_volatile(
                (crate::arch::mem::get_phys_offset() + address) as *mut u32,
                value,
            )
        }
    }

    #[inline(always)]
    fn write_u64(&self, address: usize, value: u64) {
        unsafe {
            core::ptr::write_volatile(
                (crate::arch::mem::get_phys_offset() + address) as *mut u64,
                value,
            )
        }
    }

    #[inline(always)]
    fn read_io_u8(&self, port: u16) -> u8 {
        unsafe { ReadWritePort::new(port).read() }
    }

    #[inline(always)]
    fn read_io_u16(&self, port: u16) -> u16 {
        unsafe { ReadWritePort::new(port).read() }
    }

    #[inline(always)]
    fn read_io_u32(&self, port: u16) -> u32 {
        unsafe { ReadWritePort::new(port).read() }
    }

    #[inline(always)]
    fn write_io_u8(&self, port: u16, value: u8) {
        unsafe { ReadWritePort::new(port).write(value) }
    }

    #[inline(always)]
    fn write_io_u16(&self, port: u16, value: u16) {
        unsafe { ReadWritePort::new(port).write(value) }
    }

    #[inline(always)]
    fn write_io_u32(&self, port: u16, value: u32) {
        unsafe { ReadWritePort::new(port).write(value) }
    }

    #[inline(always)]
    fn read_pci_u8(&self, address: acpi::PciAddress, offset: u16) -> u8 {
        todo!()
    }

    #[inline(always)]
    fn read_pci_u16(&self, address: acpi::PciAddress, offset: u16) -> u16 {
        todo!()
    }

    #[inline(always)]
    fn read_pci_u32(&self, address: acpi::PciAddress, offset: u16) -> u32 {
        todo!()
    }

    #[inline(always)]
    fn write_pci_u8(&self, address: acpi::PciAddress, offset: u16, value: u8) {
        todo!()
    }

    #[inline(always)]
    fn write_pci_u16(&self, address: acpi::PciAddress, offset: u16, value: u16) {
        todo!()
    }

    #[inline(always)]
    fn write_pci_u32(&self, address: acpi::PciAddress, offset: u16, value: u32) {
        todo!()
    }

    #[inline(always)]
    fn nanos_since_boot(&self) -> u64 {
        todo!()
    }

    fn stall(&self, microseconds: u64) {
        todo!()
    }

    fn sleep(&self, milliseconds: u64) {
        todo!()
    }
}
