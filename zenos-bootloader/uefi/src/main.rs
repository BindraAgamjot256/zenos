#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(mismatched_lifetime_syntaxes)]

use crate::memory_descriptor::UefiMemoryDescriptor;
use bootloader_api::info::FrameBufferInfo;
use bootloader_boot_config::BootConfig;
use bootloader_x86_64_common::{
    legacy_memory_region::LegacyFrameAllocator, Kernel, RawFrameBufferInfo, SystemInfo,
};
use core::panic;
use core::{ops::DerefMut, ptr, slice};
use uefi::boot;
use uefi::boot::allocate_pages;
use uefi::boot::exit_boot_services;
use uefi::boot::AllocateType;
use uefi::boot::ScopedProtocol;
use uefi::entry;
use uefi::mem::memory_map::MemoryMap;
use uefi::mem::memory_map::MemoryMapMut;
use uefi::mem::memory_map::MemoryType;
use uefi::table::cfg::ConfigTableEntry;
use uefi::Status;
use uefi::{
    proto::{
        console::gop::{GraphicsOutput, PixelFormat},
        media::{
            file::{File, FileAttribute, FileInfo, FileMode},
            fs::SimpleFileSystem,
        },
        ProtocolPointer,
    },
    CStr16,
};
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

mod memory_descriptor;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    main_inner()
}

fn main_inner() -> Status {
    // temporarily clone the y table for printing panics

    let kernel = load_kernel();
    if kernel.is_none() {
        panic!("We only support disk boot now.")
    }
    let kernel = kernel.expect("Failed to load kernel");

    let config = load_config().unwrap_or_default();

    let framebuffer = init_logger(&config);

    // Load and render splash screen if configured
    if let Some(framebuffer) = framebuffer {
        if let Some(splash_path) = config.splash_path {
            if let Some(splash_data) = load_file_from_boot_method(splash_path) {
                unsafe {
                    bootloader_x86_64_common::bmp::draw_bmp(splash_data, &framebuffer);
                }
                log::info!("Splash screen rendered from {}", splash_path);
            } else {
                unsafe {
                    bootloader_x86_64_common::bmp::draw_bmp(&[], &framebuffer);
                }
                log::warn!("Failed to load splash from {}", splash_path);
            }
        } else {
            unsafe {
                bootloader_x86_64_common::bmp::draw_bmp(&[], &framebuffer);
            }
            panic!("Failed to load splash");
        }
    }

    log::info!("Boot config: {:?}", config);

    log::info!("UEFI bootloader started");

    if let Some(framebuffer) = framebuffer {
        log::info!("Using framebuffer at {:#x}", framebuffer.addr);
    }

    log::info!("Trying to load ramdisk");
    // Ramdisk must load from same source, or not at all.
    let ramdisk = load_ramdisk();

    log::info!(
        "{}",
        match ramdisk {
            Some(_) => "Loaded ramdisk",
            None => "Ramdisk not found.",
        }
    );

    log::trace!("exiting boot services");
    let mut memory_map = unsafe { exit_boot_services(None) };

    memory_map.sort();

    let mut frame_allocator =
        LegacyFrameAllocator::new(memory_map.entries().copied().map(UefiMemoryDescriptor));

    let max_phys_addr = frame_allocator.max_phys_addr();
    let page_tables = create_page_tables(&mut frame_allocator, max_phys_addr, framebuffer.as_ref());
    let mut ramdisk_len = 0u64;
    let ramdisk_addr = if let Some(rd) = ramdisk {
        ramdisk_len = rd.len() as u64;
        Some(rd.as_ptr() as usize as u64)
    } else {
        None
    };
    let system_info = SystemInfo {
        framebuffer,
        rsdp_addr: { Some(PhysAddr::new(get_rsdp_addr())) },
        ramdisk_addr,
        ramdisk_len,
    };

    bootloader_x86_64_common::load_and_switch_to_kernel(
        kernel,
        config,
        frame_allocator,
        page_tables,
        system_info,
    );
    // This should never be reached, as the kernel should take over control.
}

#[derive(Clone, Copy, Debug)]
pub enum BootMode {
    Disk,
    Tftp,
}

fn load_ramdisk() -> Option<&'static mut [u8]> {
    load_file_from_boot_method("initrd\0")
}

fn load_kernel() -> Option<Kernel<'static>> {
    let kernel_slice = load_file_from_boot_method("zenos_kernel\0")?;
    Some(Kernel::parse(kernel_slice))
}

fn load_config() -> Option<BootConfig<'static>> {
    let config_slice = load_file_from_boot_method("boot_config\0")?;
    let config_str = core::str::from_utf8(config_slice).ok()?;
    Some(BootConfig::from_str(config_str))
}

fn load_file_from_boot_method(filename: &str) -> Option<&'static mut [u8]> {
    load_file_from_disk(filename)
}

fn locate_and_open_protocol<P: ProtocolPointer>() -> Option<ScopedProtocol<P>> {
    let handle = boot::get_handle_for_protocol::<P>().ok()?;
    boot::open_protocol_exclusive::<P>(handle).ok()
}

fn load_file_from_disk(name: &str) -> Option<&'static mut [u8]> {
    let mut file_system_raw = locate_and_open_protocol::<SimpleFileSystem>()?;
    let file_system = file_system_raw.deref_mut();

    let mut root = file_system.open_volume().unwrap();
    let mut buf = [0u16; 256];
    assert!(name.len() < 256);
    let filename = CStr16::from_str_with_buf(name.trim_end_matches('\0'), &mut buf)
        .expect("Failed to convert string to utf16");

    let file_handle_result = root.open(filename, FileMode::Read, FileAttribute::empty());

    let file_handle = file_handle_result.ok()?;

    let mut file = match file_handle.into_type().unwrap() {
        uefi::proto::media::file::FileType::Regular(f) => f,
        uefi::proto::media::file::FileType::Dir(_) => panic!(),
    };

    let mut buf = [0; 500];
    let file_info: &mut FileInfo = file.get_info(&mut buf).unwrap();
    let file_size = usize::try_from(file_info.file_size()).unwrap();

    let file_ptr = allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        ((file_size - 1) / 4096) + 1,
    )
    .unwrap()
    .as_ptr();
    unsafe { ptr::write_bytes(file_ptr, 0, file_size) };
    let file_slice = unsafe { slice::from_raw_parts_mut(file_ptr, file_size) };
    file.read(file_slice).unwrap();

    Some(file_slice)
}

/// Creates page table abstraction types for both the bootloader and kernel page tables.
fn create_page_tables(
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    max_phys_addr: PhysAddr,
    frame_buffer: Option<&RawFrameBufferInfo>,
) -> bootloader_x86_64_common::PageTables {
    // UEFI identity-maps all memory, so the offset between physical and virtual addresses is 0
    let phys_offset = VirtAddr::new(0);

    // copy the currently active level 4 page table, because it might be read-only
    log::trace!("switching to new level 4 table");
    let bootloader_page_table = {
        let old_table = {
            let frame = x86_64::registers::control::Cr3::read().0;
            let ptr: *const PageTable = (phys_offset + frame.start_address().as_u64()).as_ptr();
            unsafe { &*ptr }
        };
        let new_frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for new level 4 table");
        let new_table: &mut PageTable = {
            let ptr: *mut PageTable =
                (phys_offset + new_frame.start_address().as_u64()).as_mut_ptr();
            // create a new, empty page table
            unsafe {
                ptr.write(PageTable::new());
                &mut *ptr
            }
        };

        // copy the pml4 entries for all identity mapped memory.
        let end_addr = VirtAddr::new(max_phys_addr.as_u64() - 1);
        for p4 in 0..=usize::from(end_addr.p4_index()) {
            new_table[p4] = old_table[p4].clone();
        }

        // copy the pml4 entry for the frame buffer (the frame buffer is not
        // necessarily part of the identity mapping).
        if let Some(frame_buffer) = frame_buffer {
            let start_addr = VirtAddr::new(frame_buffer.addr.as_u64());
            let end_addr = start_addr + frame_buffer.info.byte_len as u64;
            for p4 in usize::from(start_addr.p4_index())..=usize::from(end_addr.p4_index()) {
                new_table[p4] = old_table[p4].clone();
            }
        }

        // the first level 4 table entry is now identical, so we can just load the new one
        unsafe {
            x86_64::registers::control::Cr3::write(
                new_frame,
                x86_64::registers::control::Cr3Flags::empty(),
            );
            OffsetPageTable::new(&mut *new_table, phys_offset)
        }
    };

    // create a new page table hierarchy for the kernel
    let (kernel_page_table, kernel_level_4_frame) = {
        // get an unused frame for new level 4 page table
        let frame: PhysFrame = frame_allocator.allocate_frame().expect("no unused frames");
        log::info!("New page table at: {:#?}", &frame);
        // get the corresponding virtual address
        let addr = phys_offset + frame.start_address().as_u64();
        // initialize a new page table
        let ptr = addr.as_mut_ptr();
        unsafe { *ptr = PageTable::new() };
        let level_4_table = unsafe { &mut *ptr };
        (
            unsafe { OffsetPageTable::new(level_4_table, phys_offset) },
            frame,
        )
    };

    bootloader_x86_64_common::PageTables {
        bootloader: bootloader_page_table,
        kernel: kernel_page_table,
        kernel_level_4_frame,
    }
}

fn init_logger(config: &BootConfig<'_>) -> Option<RawFrameBufferInfo> {
    bootloader_x86_64_common::init_logger(log::LevelFilter::Trace, true);

    let mut gop = boot::get_handle_for_protocol::<GraphicsOutput>()
        .ok()
        .and_then(|h| boot::open_protocol_exclusive::<GraphicsOutput>(h).ok())
        .unwrap();

    let mode = {
        let modes = gop.modes();
        match (
            config
                .framebuffer_height
                .map(|v: u64| usize::try_from(v).unwrap()),
            config
                .framebuffer_width
                .map(|v: u64| usize::try_from(v).unwrap()),
        ) {
            (Some(height), Some(width)) => modes
                .filter(|m| {
                    let res = m.info().resolution();
                    res.1 >= height && res.0 >= width
                })
                .last(),
            (Some(height), None) => modes.filter(|m| m.info().resolution().1 >= height).last(),
            (None, Some(width)) => modes.filter(|m| m.info().resolution().0 >= width).last(),
            _ => None,
        }
    };
    if let Some(mode) = mode {
        gop.set_mode(&mode)
            .expect("Failed to apply the desired display mode");
    }

    let mode_info = gop.current_mode_info();
    let mut framebuffer = gop.frame_buffer();
    let info = FrameBufferInfo {
        byte_len: framebuffer.size(),
        width: mode_info.resolution().0,
        height: mode_info.resolution().1,
        pixel_format: match mode_info.pixel_format() {
            PixelFormat::Rgb => bootloader_api::info::PixelFormat::Rgb,
            PixelFormat::Bgr => bootloader_api::info::PixelFormat::Bgr,
            PixelFormat::Bitmask | PixelFormat::BltOnly => {
                panic!("Bitmask and BltOnly framebuffers are not supported")
            }
        },
        bytes_per_pixel: 4,
        stride: mode_info.stride(),
    };

    Some(RawFrameBufferInfo {
        addr: PhysAddr::new(framebuffer.as_mut_ptr() as u64),
        info,
    })
}

fn get_rsdp_addr() -> u64 {
    let st = unsafe { uefi::table::system_table_raw().unwrap().as_ref() };

    let tables = st.configuration_table;
    let count = st.number_of_configuration_table_entries as usize;

    for i in 0..count {
        let table = unsafe { tables.add(i).as_ref().unwrap() };

        if table.vendor_guid == ConfigTableEntry::ACPI2_GUID {
            return table.vendor_table as u64;
        }
    }

    for i in 0..count {
        let table = unsafe { tables.add(i).as_ref().unwrap() };

        if table.vendor_guid == ConfigTableEntry::ACPI_GUID {
            return table.vendor_table as u64;
        }
    }

    panic!("RSDP not found");
}

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::arch::asm;

    unsafe {
        bootloader_x86_64_common::logger::LOGGER
            .get()
            .map(|l| l.force_unlock())
    };
    log::error!("{}", info);

    loop {
        unsafe { asm!("cli; hlt") };
    }
}
