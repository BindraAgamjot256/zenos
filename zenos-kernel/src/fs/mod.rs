//todo: docs, nvme, tests
use crate::memory::{
    KERNEL_BASE, PAGE_4K, PageType, kalloc_dma_pages, kalloc_page, kfree_dma_pages, kfree_page,
};
use crate::pci::scan_pci_for_ahci;
use alloc::boxed::Box;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{Ordering, compiler_fence};
use fatfs::{FileSystem, IoBase, Read, Seek, SeekFrom, Write};
use heapless::Vec;
use log::{debug, error, trace};
use spin::{Lazy, Mutex};
use x86_64::{PhysAddr, VirtAddr};

// ============================================================================
// Constants
// ============================================================================

const AHCI_VIRT_BASE: u64 = KERNEL_BASE + 0x2000_0000;
const SECTOR_SIZE: usize = 512;
const MAX_SLOTS: usize = 32;
const TIMEOUT_MAX: u32 = 10_000_000;

// AHCI Register Offsets
mod reg {
    pub const PORTS_IMPLEMENTED: usize = 0x0C;
    pub const PORT_BASE: usize = 0x100;
    pub const PORT_SIZE: usize = 0x80;

    // Port-specific offsets (from port base)
    pub const CLB: usize = 0x00;
    pub const CLBU: usize = 0x04;
    pub const FB: usize = 0x08;
    pub const FBU: usize = 0x0C;
    pub const IS: usize = 0x10;
    pub const TFD: usize = 0x20;
    pub const SSTS: usize = 0x28;
    pub const SERR: usize = 0x30;
    pub const CI: usize = 0x38;
    pub const CMD: usize = 0x18;
}

// AHCI Command Codes
mod cmd {
    pub const READ_DMA_EXT: u8 = 0x25;
    pub const WRITE_DMA_EXT: u8 = 0x35;
}

// Bit flags
mod flags {
    pub const CMD_ST: u32 = 1 << 0;
    pub const CMD_FRE: u32 = 1 << 4;
    pub const CMD_FR: u32 = 1 << 14;
    pub const CMD_CR: u32 = 1 << 15;
    pub const IS_TFES: u32 = 1 << 30;
    pub const TFD_ERR: u32 = 1 << 0;
    pub const PRDT_IOC: u32 = 1 << 31;
    pub const CMD_WRITE: u16 = 1 << 6;
    pub const DEVICE_LBA: u8 = 1 << 6;
}

// ============================================================================
// Data Structures
// ============================================================================

#[repr(C)]
struct HbaCmdHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    _rsv: [u32; 4],
}

impl HbaCmdHeader {
    fn set_command_table_addr(&mut self, addr: PhysAddr) {
        self.ctba = addr.as_u64() as u32;
        self.ctbau = (addr.as_u64() >> 32) as u32;
    }

    fn command_table_addr(&self) -> u64 {
        (self.ctba as u64) | ((self.ctbau as u64) << 32)
    }
}

#[repr(C)]
struct HbaPrdtEntry {
    dba: u32,
    dbau: u32,
    rsv0: u32,
    dbc: u32,
}

impl HbaPrdtEntry {
    fn set_data_addr(&mut self, addr: PhysAddr) {
        self.dba = (addr.as_u64() & 0xFFFF_FFFF) as u32;
        self.dbau = ((addr.as_u64() >> 32) & 0xFFFF_FFFF) as u32;
    }

    fn set_byte_count(&mut self, bytes: u32, interrupt_on_completion: bool) {
        self.dbc = (bytes - 1)
            | if interrupt_on_completion {
                flags::PRDT_IOC
            } else {
                0
            };
    }
}

#[repr(C)]
struct RegFis {
    fis_type: u8,
    pmport: u8,
    command: u8,
    featurel: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    featureh: u8,
    countl: u8,
    counth: u8,
    icc: u8,
    control: u8,
    rsv1: [u8; 4],
}

impl RegFis {
    fn new_dma_command(lba: u64, count: u16, write: bool) -> Self {
        Self {
            fis_type: 0x27, // Register Host to Device
            pmport: 0x80,   // Bit 7: Command register update
            command: if write {
                cmd::WRITE_DMA_EXT
            } else {
                cmd::READ_DMA_EXT
            },
            device: flags::DEVICE_LBA,
            lba0: (lba & 0xFF) as u8,
            lba1: ((lba >> 8) & 0xFF) as u8,
            lba2: ((lba >> 16) & 0xFF) as u8,
            lba3: ((lba >> 24) & 0xFF) as u8,
            lba4: ((lba >> 32) & 0xFF) as u8,
            lba5: ((lba >> 40) & 0xFF) as u8,
            countl: (count & 0xFF) as u8,
            counth: ((count >> 8) & 0xFF) as u8,
            featurel: 0,
            featureh: 0,
            icc: 0,
            control: 0,
            rsv1: [0; 4],
        }
    }
}

#[repr(C, align(128))]
struct HbaCmdTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    rsv: [u8; 48],
    prdt_entry: [HbaPrdtEntry; 8],
}

// ============================================================================
// Port & PortInfo
// ============================================================================

#[derive(Copy, Clone)]
struct PortInfo {
    port_base: usize, // MMIO base as usize (the pointer value you wrote into PORT regs)
    virt_cmd_list: VirtAddr, // virtual address where command list resides (for CPU access)
    virt_cmd_tables_base: VirtAddr, // base virtual address for per-slot command tables
}

struct Port {
    base: *mut u32,
    info: PortInfo,
}

impl Port {
    unsafe fn new(info: PortInfo) -> Self {
        Self {
            base: info.port_base as *mut u32,
            info,
        }
    }

    unsafe fn read_reg(&self, offset: usize) -> u32 {
        read_volatile(self.base.add(offset / 4))
    }

    unsafe fn write_reg(&self, offset: usize, value: u32) {
        write_volatile(self.base.add(offset / 4), value)
    }

    unsafe fn stop(&self) {
        // Stop command engine
        let mut cmd = self.read_reg(reg::CMD);
        cmd &= !flags::CMD_ST;
        self.write_reg(reg::CMD, cmd);

        // Wait until command list is no longer running
        while self.read_reg(reg::CMD) & flags::CMD_CR != 0 {
            core::hint::spin_loop();
        }

        // Stop FIS receive
        cmd = self.read_reg(reg::CMD);
        cmd &= !flags::CMD_FRE;
        self.write_reg(reg::CMD, cmd);

        // Wait until FIS receive is no longer running
        while self.read_reg(reg::CMD) & flags::CMD_FR != 0 {
            core::hint::spin_loop();
        }
    }

    unsafe fn start(&self) {
        // Start FIS receive
        let mut cmd = self.read_reg(reg::CMD);
        cmd |= flags::CMD_FRE;
        self.write_reg(reg::CMD, cmd);

        // Start command engine
        cmd = self.read_reg(reg::CMD);
        cmd |= flags::CMD_ST;
        self.write_reg(reg::CMD, cmd);
    }

    unsafe fn has_drive(&self) -> bool {
        let ssts = self.read_reg(reg::SSTS);
        let det = ssts & 0xF;
        let ipm = (ssts >> 8) & 0xF;
        det == 3 && ipm == 1
    }

    unsafe fn allocate_slot(&self) -> Option<u8> {
        let ci = self.read_reg(reg::CI);
        (0..MAX_SLOTS as u8).find(|&slot| ci & (1 << slot) == 0)
    }

    unsafe fn clear_errors(&self) {
        self.write_reg(reg::IS, 0xFFFF_FFFF);
        self.write_reg(reg::SERR, 0xFFFF_FFFF);
    }

    // helper to mask DBC to AHCI spec (bits 0..21 hold byte count-1)
    fn prdt_mask_bytecount(bytes_minus_one: u32) -> u32 {
        bytes_minus_one & 0x003F_FFFF
    }

    unsafe fn send_command(
        &self,
        lba: u64,
        buf: *mut u8,
        sector_count: u16,
        write: bool,
    ) -> Result<(), ()> {
        self.clear_errors();

        // Use the virtual command list base that we mapped earlier.
        let cmd_headers = self.info.virt_cmd_list.as_ptr::<HbaCmdHeader>() as *mut HbaCmdHeader;

        let slot = self.allocate_slot().ok_or_else(|| {
            error!("No available command slots");
            ()
        })?;

        let header = &mut *cmd_headers.add(slot as usize);

        // Configure command header (we may update prdtl later)
        let fis_size = (size_of::<RegFis>() / 4) as u16;
        header.flags = (fis_size & 0x1F) | if write { flags::CMD_WRITE } else { 0 };
        header.prdtl = 0; // set after PRDT prepared
        header.prdbc = 0;

        // Get command table via the mapped virtual base for command tables
        let table_virt =
            VirtAddr::new(self.info.virt_cmd_tables_base.as_u64() + (slot as u64) * PAGE_4K as u64);
        let table = &mut *(table_virt.as_mut_ptr::<HbaCmdTable>());

        // Setup FIS
        let fis = RegFis::new_dma_command(lba, sector_count, write);
        let fis_ptr = table.cfis.as_mut_ptr() as *mut RegFis;
        write_volatile(fis_ptr, fis);

        // --- ALLOCATE A PINNED DMA PAGE (safe) ---
        let total_bytes = sector_count as usize * SECTOR_SIZE;
        if total_bytes == 0 || total_bytes > PAGE_4K as usize {
            error!("Unsupported transfer size: {} bytes", total_bytes);
            return Err(());
        }

        // choose a deterministic virt address for DMA pages (per-slot)
        let dma_page_virt = VirtAddr::new(AHCI_VIRT_BASE + 0x8000 + (slot as u64) * PAGE_4K as u64);
        let dma_page_phys =
            crate::memory::virt_to_phys(dma_page_virt).expect("Failed to map DMA page");

        // zero it - safer
        core::ptr::write_bytes(dma_page_virt.as_mut_ptr::<u8>(), 0, PAGE_4K);

        // copy user->dma page for writes
        if write {
            core::ptr::copy_nonoverlapping(buf, dma_page_virt.as_mut_ptr::<u8>(), total_bytes);
        }

        // Setup PRDT (single-entry)
        let prdt = &mut table.prdt_entry[0];
        prdt.set_data_addr(dma_page_phys);
        let dbc_masked = Self::prdt_mask_bytecount((total_bytes as u32).wrapping_sub(1));
        prdt.dbc = dbc_masked | flags::PRDT_IOC;
        header.prdtl = 1;

        // DIAGNOSTIC / TRACE
        trace!(
            "PRDT: dba={:#x}, dbau={:#x}, dbc={:#x}, total_bytes={}",
            prdt.dba, prdt.dbau, prdt.dbc, total_bytes
        );
        trace!(
            "table_virt={:#x}, table_phys_expected={:#x}, header_ctba={:#x}",
            table_virt.as_u64(),
            // we expect the header's ctba to have been set in init(); show it for debug
            header.command_table_addr(),
            header.command_table_addr()
        );
        trace!(
            "dma_page_virt={:#x}, dma_page_phys={:#x}, buf_virt={:#x}",
            dma_page_virt.as_u64(),
            dma_page_phys.as_u64(),
            buf as usize
        );

        compiler_fence(Ordering::Release);

        // ensure command engine running
        let cmd_status = self.read_reg(reg::CMD);
        trace!("Port CMD register before issue: {:#x}", cmd_status);
        if cmd_status & flags::CMD_ST == 0 {
            error!("Port command engine not running!");
            return Err(());
        }

        // Issue the command
        self.write_reg(reg::CI, 1 << slot);
        trace!("Command issued on slot {}", slot);

        // Wait for completion
        let res = self.wait_for_completion(slot);

        compiler_fence(Ordering::Acquire);

        // on success & read: copy DMA page -> destination buffer
        if res.is_ok() && !write {
            core::ptr::copy_nonoverlapping(dma_page_virt.as_ptr::<u8>(), buf, total_bytes);
        }

        // optional: free dma page here if you have a freeing API (kfree_page).
        // otherwise keep allocated for reuse.

        // post-check: TFD
        let tfd = self.read_reg(reg::TFD);
        if tfd & flags::TFD_ERR != 0 {
            let is = self.read_reg(reg::IS);
            let serr = self.read_reg(reg::SERR);
            error!(
                "AHCI command error: TFD={:#x}, IS={:#x}, SERR={:#x}, slot={}",
                tfd, is, serr, slot
            );
            // best effort to acknowledge
            self.write_reg(reg::IS, is);
            self.write_reg(reg::SERR, serr);
            return Err(());
        }

        res
    }

    unsafe fn wait_for_completion(&self, slot: u8) -> Result<(), ()> {
        for _ in 0..TIMEOUT_MAX {
            let ci = self.read_reg(reg::CI);
            if ci & (1 << slot) == 0 {
                return Ok(());
            }

            // Check for task file error
            let is = self.read_reg(reg::IS);
            if is & flags::IS_TFES != 0 {
                let serr = self.read_reg(reg::SERR);
                error!(
                    "AHCI task file error: IS={:#x}, SERR={:#x}, slot={}",
                    is, serr, slot
                );

                self.write_reg(reg::IS, is);
                self.write_reg(reg::SERR, serr);

                return Err(());
            }

            core::hint::spin_loop();
        }

        error!("AHCI command timeout on slot {}", slot);
        Err(())
    }

    unsafe fn read_sectors(&self, lba: u64, buf: *mut u8, count: u16) -> Result<(), ()> {
        self.send_command(lba, buf, count, false)
    }

    unsafe fn write_sectors(&self, lba: u64, buf: *mut u8, count: u16) -> Result<(), ()> {
        self.send_command(lba, buf, count, true)
    }
}

// ============================================================================
// Global State
// ============================================================================

// Now store PortInfo objects so we can access virtual mapping info later.
static PORTS: Mutex<Option<Vec<PortInfo, 32>>> = Mutex::new(None);

// ============================================================================
// Initialization
// ============================================================================

pub(crate) unsafe fn init() {
    let pci = scan_pci_for_ahci().expect("No AHCI controller found");

    // Map AHCI MMIO region
    let mmio_base = kalloc_page(VirtAddr::new(pci.bar5 as u64), PageType::Mmio)
        .expect("Failed to map AHCI MMIO")
        .as_u64();

    let mut ports = Vec::<_, 32>::new();
    let ports_implemented = read_volatile((mmio_base + reg::PORTS_IMPLEMENTED as u64) as *mut u32);

    for port_num in 0..MAX_SLOTS {
        if ports_implemented & (1 << port_num) == 0 {
            continue;
        }

        debug!("Initializing port {}", port_num);

        let port_base =
            (mmio_base + (reg::PORT_BASE + port_num * reg::PORT_SIZE) as u64) as *mut u32;
        // We'll store the pointer value as usize in PortInfo.port_base
        let port_base_usize = port_base as usize;
        let port = Port::new(PortInfo {
            port_base: port_base_usize,
            virt_cmd_list: VirtAddr::new(0), // temporary filler; we'll set below
            virt_cmd_tables_base: VirtAddr::new(0),
        });

        port.stop();

        // Allocate command list and FIS virtual addresses (we choose specific virtual addresses)
        let virt_cmd_list = VirtAddr::new(AHCI_VIRT_BASE + port_num as u64 * PAGE_4K as u64);
        let virt_fis = VirtAddr::new(AHCI_VIRT_BASE + 0x1000 + port_num as u64 * PAGE_4K as u64);

        // Allocate physical pages for those virtual addresses (kalloc_page will map virt->phys)
        let cmd_list_phys = kalloc_page(virt_cmd_list, PageType::Recursive)
            .expect("Failed to allocate command list");
        let fis_phys = kalloc_page(virt_fis, PageType::Recursive).expect("Failed to allocate FIS");

        // Zero out command list and FIS memory (using the virtual addresses we just reserved)
        core::ptr::write_bytes(virt_cmd_list.as_mut_ptr::<u8>(), 0, PAGE_4K as usize);
        core::ptr::write_bytes(virt_fis.as_mut_ptr::<u8>(), 0, PAGE_4K as usize);

        // Configure command list and FIS addresses in controller (controller expects physical addresses)
        port.write_reg(reg::CLB, cmd_list_phys.as_u64() as u32);
        port.write_reg(reg::CLBU, (cmd_list_phys.as_u64() >> 32) as u32);
        port.write_reg(reg::FB, fis_phys.as_u64() as u32);
        port.write_reg(reg::FBU, (fis_phys.as_u64() >> 32) as u32);

        // Allocate command tables: choose contiguous virtual area per-port (slot * PAGE_4K)
        let cmd_table_base_virt =
            VirtAddr::new(AHCI_VIRT_BASE + 0x2000 + (port_num * MAX_SLOTS) as u64 * PAGE_4K as u64);
        // allocated by bootloader, so we can use it as a pointer.
        // cmd_headers pointer to the virtual command list we just mapped
        let cmd_headers = virt_cmd_list.as_u64() as *mut HbaCmdHeader;

        for slot in 0..MAX_SLOTS {
            let table_virt =
                VirtAddr::new(cmd_table_base_virt.as_u64() + (slot as u64) * PAGE_4K as u64);
            let table_phys = kalloc_page(table_virt, PageType::Recursive)
                .expect("Failed to allocate command table");

            // Zero out command table using the virtual address
            core::ptr::write_bytes(table_virt.as_mut_ptr::<u8>(), 0, PAGE_4K);

            // Set the command table physical address into the command header (controller needs physical)
            (*cmd_headers.add(slot)).set_command_table_addr(table_phys);
        }

        port.start();

        if port.has_drive() {
            debug!("Drive detected on port {}", port_num);

            // Build PortInfo including virtual addresses we mapped.
            let pinfo = PortInfo {
                port_base: port_base_usize,
                virt_cmd_list,
                virt_cmd_tables_base: cmd_table_base_virt,
            };

            let _ = ports.push(pinfo);

            // FIXME: Multiple ports not supported yet; stop after first found
            break;
        } else {
            for slot in 0..MAX_SLOTS {
                let table_virt =
                    VirtAddr::new(cmd_table_base_virt.as_u64() + (slot as u64) * PAGE_4K as u64);
                kfree_page(table_virt, PageType::Recursive).expect("Failed to free command table");
            }
        }
    }

    *PORTS.lock() = Some(ports);
}

// ============================================================================
// Block Device Implementation
// ============================================================================
pub trait BlockDevice: Read + Write + Seek + IoBase {
    fn block_size(&self) -> u64;
}

pub struct BlockDeviceDriver<E> {
    device: Box<(dyn BlockDevice<Error = E> + Send + Sync)>,
}

impl<E> BlockDeviceDriver<E> {
    pub fn new(device: Box<(dyn BlockDevice<Error = E> + Send + Sync)>) -> Self {
        Self { device }
    }
}

impl<E: fatfs::IoError> IoBase for BlockDeviceDriver<E> {
    type Error = E;
}

impl<E: fatfs::IoError> Read for BlockDeviceDriver<E> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.device.read(buf)
    }
}

impl<E: fatfs::IoError> Write for BlockDeviceDriver<E> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.device.write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.device.flush()
    }
}

impl<E: fatfs::IoError> Seek for BlockDeviceDriver<E> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.device.seek(pos)
    }
}

pub struct AhciBlockDevice {
    port_base: usize,
    sector_size: usize,
    cursor: u64,
    partition_offset: u64, // LBA where FAT starts
}

impl BlockDevice for AhciBlockDevice {
    fn block_size(&self) -> u64 {
        self.sector_size as u64
    }
}

impl AhciBlockDevice {
    pub fn new(port_index: usize) -> Option<Self> {
        let ports = PORTS.lock();
        let ports = ports.as_ref()?;

        ports.get(port_index).map(|pinfo| Self {
            port_base: pinfo.port_base,
            sector_size: SECTOR_SIZE,
            cursor: 0,
            partition_offset: 34, // todo: get from partition table, currently always 34(sizeof GPT), panics otherwise.
        })
    }

    fn sectors_for_bytes(&self, bytes: usize) -> u16 {
        ((bytes + self.sector_size - 1) / self.sector_size) as u16
    }
}

impl IoBase for AhciBlockDevice {
    type Error = ();
}

impl Seek for AhciBlockDevice {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.cursor = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(_) => return Err(()), // TODO: Need disk size
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    self.cursor.checked_sub(offset.unsigned_abs()).ok_or(())?
                } else {
                    self.cursor.checked_add(offset as u64).ok_or(())?
                }
            }
        };
        Ok(self.cursor)
    }
}

impl Read for AhciBlockDevice {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Calculate sector-aligned buffer size
        let start_lba = (self.cursor / SECTOR_SIZE as u64) + self.partition_offset;
        let sector_offset = (self.cursor % SECTOR_SIZE as u64) as usize;
        let total_bytes_needed = sector_offset + buf.len();
        let sectors = self.sectors_for_bytes(total_bytes_needed);
        let sector_aligned_size = sectors as usize * SECTOR_SIZE;

        let ubuf = kalloc_dma_pages(sector_aligned_size).map_err(|_| ())?;
        if ubuf.is_empty() {
            return Ok(0);
        }

        let ports = PORTS.lock();
        let pinfo = match ports
            .as_ref()
            .and_then(|v| v.iter().filter(|p| p.port_base == self.port_base).next())
        {
            Some(p) => *p,
            None => return Err(()),
        };
        let port = unsafe { Port::new(pinfo) };

        unsafe {
            port.read_sectors(start_lba, ubuf.as_mut_ptr(), sectors)?;
        }

        // Calculate how many bytes we can actually copy
        let available_bytes = ubuf.len().saturating_sub(sector_offset);
        let bytes_to_copy = buf.len().min(available_bytes);

        // Copy from the correct offset in the sector-aligned buffer
        buf[..bytes_to_copy].copy_from_slice(&ubuf[sector_offset..sector_offset + bytes_to_copy]);

        self.cursor += bytes_to_copy as u64; // Increment by BYTES, not sectors
        kfree_dma_pages(ubuf).map_err(|_| ())?;
        Ok(bytes_to_copy)
    }
}

impl Write for AhciBlockDevice {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        // For writes, we need to handle partial sectors by reading-modifying-writing
        let start_lba = (self.cursor / SECTOR_SIZE as u64) + self.partition_offset;
        let sector_offset = (self.cursor % SECTOR_SIZE as u64) as usize;

        let total_bytes_needed = sector_offset + buf.len();
        let sectors = self.sectors_for_bytes(total_bytes_needed);

        let ubuf = kalloc_dma_pages(sectors as usize * SECTOR_SIZE).map_err(|_| ())?;

        // If we're not writing full sectors, read existing data first
        if sector_offset != 0 || buf.len() % SECTOR_SIZE != 0 {
            let ports = PORTS.lock();
            let pinfo = match ports.as_ref().and_then(|v| v.get(0)) {
                Some(p) => *p,
                None => return Err(()),
            };
            let port = unsafe { Port::new(pinfo) };

            unsafe {
                port.read_sectors(start_lba, ubuf.as_mut_ptr(), sectors)?;
            }
        }

        // Copy new data into the buffer
        ubuf[sector_offset..sector_offset + buf.len()].copy_from_slice(buf);

        let ports = PORTS.lock();
        let pinfo = match ports.as_ref().and_then(|v| v.get(0)) {
            Some(p) => *p,
            None => return Err(()),
        };
        let port = unsafe { Port::new(pinfo) };

        unsafe {
            port.write_sectors(start_lba, ubuf.as_mut_ptr(), sectors)?;
        }

        self.cursor += buf.len() as u64; // Increment by BYTES written
        kfree_dma_pages(ubuf).map_err(|_| ())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub static FS: Lazy<Mutex<FileSystem<BlockDeviceDriver<()>>>> = Lazy::new(|| {
    unsafe { init() }
    let fs = FileSystem::new(
        BlockDeviceDriver::new(Box::new(
            AhciBlockDevice::new(0).expect("Port 0 unavailable"),
        )),
        fatfs::FsOptions::new(),
    )
    .expect("Panics");

    Mutex::new(fs)
});
