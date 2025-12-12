//! AHCI (SATA) block device driver.
//!
//! This module provides a minimal AHCI implementation sufficient for sector
//! reads/writes via DMA, exposing a [`BlockDevice`] implementation
//! ([`AhciBlockDevice`]) that can be consumed by the filesystem layer.
//!
//! # Theory of Operation
//!
//! AHCI (Advanced Host Controller Interface) acts as a bridge between the system memory
//! and SATA devices. Communication happens via **Command Lists** and **Command Tables**
//! located in system memory, which the HBA (Host Bus Adapter) reads via DMA.
//!
//! The flow of a single IO operation is:
//! 1. **Preparation**: The CPU builds a Command Table containing a FIS (Frame Information Structure)
//!    and a PRDT (Physical Region Descriptor Table). The PRDT points to the data buffer.
//! 2. **Submission**: The CPU updates the Command List Header to point to this table and
//!    sets a bit in the Port's Command Issue (`CI`) register (the "doorbell").
//! 3. **Execution**: The HBA detects the set bit, fetches the command via DMA, transmits
//!    it to the drive, transfers data, and updates the status.
//! 4. **Completion**: The HBA clears the bit in the `CI` register (and optionally raises an interrupt).
//!    The CPU detects this (via polling in this driver) to mark the IO as complete.
//!
//! # Memory Layout
//!
//! AHCI requires several distinct memory structures for every port:
//! - **Command List**: Array of 32 headers (one per slot).
//! - **FIS Receive Area**: Buffer where the HBA writes incoming status FIS.
//! - **Command Tables**: One per slot. Contains the actual ATA command and scatter/gather list.
//!
//! Design highlights:
//! - We map the AHCI HBA MMIO BAR and a small set of per-port structures
//!   at fixed virtual addresses.
//! - Only a single port is initialized and exposed (first detected with a
//!   drive). Multi-port/NCQ are not implemented yet.
//! - I/O is synchronous: we submit a command and busy-wait for completion with
//!   a timeout.
//!
//! # Safety
//!
//! This module is heavily `unsafe` as it deals with:
//! - Raw MMIO pointers.
//! - Physical memory addresses (for DMA).
//! - Volatile reads/writes to hardware registers.
//!
//! The safe interface is provided via [`AhciBlockDevice`].

use crate::disk::block::BlockDevice;
use crate::disk::FileError;
use crate::memory::{
    kalloc_dma_pages, kalloc_page, kfree_dma_pages, kfree_page, PageType, KERNEL_BASE, PAGE_4K,
};
use crate::pci::scan_pci_for_ahci;
use alloc::boxed::Box;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering};
use fatfs::{IoBase, Read, Seek, SeekFrom, Write};
use heapless::Vec;
use log::{debug, error, trace};
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

// Fixed virtual address where we map the AHCI structures (Command Lists, etc.)
const AHCI_VIRT_BASE: u64 = KERNEL_BASE + 0x2000_0000;
const SECTOR_SIZE: usize = 512;
const MAX_SLOTS: usize = 32;
// Approx loop count for timeout; depends on CPU speed, should be replaced by timer ticks in real OS
const TIMEOUT_MAX: u32 = 10_000_000;

/// AHCI Generic Host Control Registers and Port Register Offsets.
mod reg {
    pub const PORTS_IMPLEMENTED: usize = 0x0C; // PI: Bitmap of available ports
    pub const PORT_BASE: usize = 0x100; // Offset of Port 0
    pub const PORT_SIZE: usize = 0x80; // Size of one port's register bank

    // Port-specific offsets (from port base)
    pub const CLB: usize = 0x00; // Command List Base Address (Low)
    pub const CLBU: usize = 0x04; // Command List Base Address (High)
    pub const FB: usize = 0x08; // FIS Base Address (Low)
    pub const FBU: usize = 0x0C; // FIS Base Address (High)
    pub const IS: usize = 0x10; // Interrupt Status
    pub const CMD: usize = 0x18; // Command and Status
    pub const TFD: usize = 0x20; // Task File Data (Status/Error from drive)
    pub const SSTS: usize = 0x28; // Serial ATA Status (SCR0: SStatus)
    pub const SERR: usize = 0x30; // Serial ATA Error (SCR1: SError)
    pub const CI: usize = 0x38; // Command Issue (Bitmap of pending commands)
}

/// ATA Command Codes (sent inside the FIS).
mod cmd {
    pub const READ_DMA_EXT: u8 = 0x25; // Read data from disk (48-bit LBA)
    pub const WRITE_DMA_EXT: u8 = 0x35; // Write data to disk (48-bit LBA)
}

/// Bit flags for various registers and structures.
mod flags {
    // Port Command Register (PxCMD)
    pub const CMD_ST: u32 = 1 << 0; // Start (1 = Process command list)
    pub const CMD_FRE: u32 = 1 << 4; // FIS Receive Enable
    pub const CMD_FR: u32 = 1 << 14; // FIS Receive Running (Read only)
    pub const CMD_CR: u32 = 1 << 15; // Command List Running (Read only)

    // Interrupt Status
    pub const IS_TFES: u32 = 1 << 30; // Task File Error Status

    // Task File Data
    pub const TFD_ERR: u32 = 1 << 0; // Error bit in Status register

    // PRDT (Scatter/Gather) Flags
    pub const PRDT_IOC: u32 = 1 << 31; // Interrupt On Completion

    // Command Header Flags
    pub const CMD_WRITE: u16 = 1 << 6; // Direction: Write (Host to Device)

    // FIS Device Register
    pub const DEVICE_LBA: u8 = 1 << 6; // LBA Mode enable (vs CHS)
}

// ============================================================================
// Data Structures (Hardware Layout)
// ============================================================================

/// **Command List Header**.
/// There are 32 of these per port. Each points to a [`HbaCmdTable`] in memory.
/// The HBA reads this to know where the detailed command data is.
#[repr(C)]
struct HbaCmdHeader {
    // DW0
    flags: u16, // Command FIS length, Write bit, Prefetchable, etc.
    prdtl: u16, // Physical Region Descriptor Table Length (entry count)
    // DW1
    prdbc: u32, // Transferred byte count (status field written by HBA)
    // DW2, DW3
    ctba: u32,  // Command Table Descriptor Base Address (Lower 32 bits)
    ctbau: u32, // Command Table Descriptor Base Address (Upper 32 bits)
    // DW4-7
    _rsv: [u32; 4],
}

impl HbaCmdHeader {
    /// Helper to store the 64-bit physical address of the Command Table.
    fn set_command_table_addr(&mut self, addr: PhysAddr) {
        self.ctba = addr.as_u64() as u32;
        self.ctbau = (addr.as_u64() >> 32) as u32;
    }

    /// Debug helper to retrieve the address.
    fn command_table_addr(&self) -> u64 {
        (self.ctba as u64) | ((self.ctbau as u64) << 32)
    }
}

/// **Physical Region Descriptor Table (PRDT) Entry**.
/// This is a scatter/gather list entry used for DMA. It tells the HBA where
/// to read/write data in system memory.
#[repr(C)]
struct HbaPrdtEntry {
    dba: u32,  // Data Base Address (Lower 32 bits)
    dbau: u32, // Data Base Address (Upper 32 bits)
    rsv0: u32, // Reserved
    dbc: u32,  // Data Byte Count (bit 0-21) + Interrupt Flag (bit 31)
}

impl HbaPrdtEntry {
    fn set_data_addr(&mut self, addr: PhysAddr) {
        self.dba = (addr.as_u64() & 0xFFFF_FFFF) as u32;
        self.dbau = ((addr.as_u64() >> 32) & 0xFFFF_FFFF) as u32;
    }
}

/// **FIS (Frame Information Structure) - Host to Device**.
/// This structure represents the actual ATA command packet sent over the wire.
/// Specifically, this maps to `FIS Type 27h` (Register - Host to Device).
#[repr(C)]
struct RegFis {
    fis_type: u8, // Must be 0x27
    pmport: u8,   // Port multiplier + Command bit (Bit 7 must be 1)
    command: u8,  // ATA Command Code (e.g., READ_DMA_EXT)
    featurel: u8, // Features Low

    // LBA (Logical Block Address) 48-bit breakdown
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8, // Device register (LBA mode bit)
    lba3: u8,
    lba4: u8,
    lba5: u8,
    featureh: u8, // Features High

    countl: u8,  // Sector Count Low
    counth: u8,  // Sector Count High
    icc: u8,     // Isochronous Command Completion
    control: u8, // Control Register
    rsv1: [u8; 4],
}

impl RegFis {
    /// Constructs a standard DMA read/write command FIS.
    fn new_dma_command(lba: u64, count: u16, write: bool) -> Self {
        Self {
            fis_type: 0x27, // Register Host to Device
            pmport: 0x80,   // Bit 7 set: Update Command Register (execute immediately)
            command: if write {
                cmd::WRITE_DMA_EXT
            } else {
                cmd::READ_DMA_EXT
            },
            device: flags::DEVICE_LBA, // Enable LBA mode
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

/// **Command Table**.
/// Pointed to by the Command Header. Contains the FIS to send and the PRDT entries.
/// Must be 128-byte aligned.
#[repr(C, align(128))]
struct HbaCmdTable {
    cfis: [u8; 64],                // Command FIS (copied from RegFis)
    acmd: [u8; 16],                // ATAPI Command (SCSI packet), unused for HDD
    rsv: [u8; 48],                 // Reserved
    prdt_entry: [HbaPrdtEntry; 8], // Scatter/Gather list (we only use 1 entry for simplicity)
}

// ============================================================================
// Port & PortInfo
// ============================================================================

/// Metadata about an initialized port to reconstruct the [`Port`] struct later.
#[derive(Copy, Clone)]
struct PortInfo {
    port_base: usize,               // MMIO base address (Virtual)
    virt_cmd_list: VirtAddr,        // Virtual address of Command List (for CPU access)
    virt_cmd_tables_base: VirtAddr, // Virtual base address for per-slot Command Tables
}

/// Represents an active AHCI Port.
/// Provides methods to manipulate port registers and issue commands.
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

    /// Read a register at `offset` relative to this port's base.
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        read_volatile(self.base.add(offset / 4))
    }

    /// Write `value` to a register at `offset`.
    unsafe fn write_reg(&self, offset: usize, value: u32) {
        write_volatile(self.base.add(offset / 4), value)
    }

    /// Stops the port's command engine and FIS receive engine.
    /// Necessary before reconfiguring the command list addresses.
    unsafe fn stop(&self) {
        // 1. Clear Start bit
        let mut cmd = self.read_reg(reg::CMD);
        cmd &= !flags::CMD_ST;
        self.write_reg(reg::CMD, cmd);

        // 2. Wait for Command Running (CR) bit to clear
        while self.read_reg(reg::CMD) & flags::CMD_CR != 0 {
            core::hint::spin_loop();
        }

        // 3. Clear FIS Receive Enable bit
        cmd = self.read_reg(reg::CMD);
        cmd &= !flags::CMD_FRE;
        self.write_reg(reg::CMD, cmd);

        // 4. Wait for FIS Receive Running (FR) bit to clear
        while self.read_reg(reg::CMD) & flags::CMD_FR != 0 {
            core::hint::spin_loop();
        }
    }

    /// Starts the port's command engine.
    unsafe fn start(&self) {
        // 1. Enable FIS Receive
        let mut cmd = self.read_reg(reg::CMD);
        cmd |= flags::CMD_FRE;
        self.write_reg(reg::CMD, cmd);

        // 2. Enable Command Processing
        cmd = self.read_reg(reg::CMD);
        cmd |= flags::CMD_ST;
        self.write_reg(reg::CMD, cmd);
    }

    /// Checks hardware status to see if a drive is physically present and active.
    unsafe fn has_drive(&self) -> bool {
        let ssts = self.read_reg(reg::SSTS);
        let det = ssts & 0xF; // Device Detection (3 = Present and Comm established)
        let ipm = (ssts >> 8) & 0xF; // Interface Power Management (1 = Active)
        det == 3 && ipm == 1
    }

    /// Finds a free command slot (0-31) by checking the Command Issue (CI) bitmap.
    unsafe fn allocate_slot(&self) -> Option<u8> {
        let ci = self.read_reg(reg::CI);
        // Find first bit that is 0 (idle)
        (0..MAX_SLOTS as u8).find(|&slot| ci & (1 << slot) == 0)
    }

    /// Clears pending error bits in Interrupt Status and SError registers.
    unsafe fn clear_errors(&self) {
        self.write_reg(reg::IS, 0xFFFF_FFFF); // Write 1 to clear
        self.write_reg(reg::SERR, 0xFFFF_FFFF); // Write 1 to clear
    }

    /// Helper to mask DBC to AHCI spec (bits 0..21 hold byte count-1).
    fn prdt_mask_bytecount(bytes_minus_one: u32) -> u32 {
        bytes_minus_one & 0x003F_FFFF
    }

    /// Core logic to issue a read/write command.
    ///
    /// # Arguments
    /// * `lba`: 48-bit Logical Block Address.
    /// * `buf`: Destination/Source buffer (Virtual Address).
    /// * `sector_count`: Number of sectors.
    /// * `write`: True for write, False for read.
    unsafe fn send_command(
        &self,
        lba: u64,
        buf: *mut u8,
        sector_count: u16,
        write: bool,
    ) -> Result<(), ()> {
        self.clear_errors();

        // 1. Get access to the Command Headers (Virtual Address)
        let cmd_headers = self.info.virt_cmd_list.as_ptr::<HbaCmdHeader>() as *mut HbaCmdHeader;

        // 2. Find a free slot to use
        let slot = self.allocate_slot().ok_or_else(|| {
            error!("No available command slots");
        })?;

        // 3. Configure the Command Header for this slot
        let header = &mut *cmd_headers.add(slot as usize);
        let fis_size = (size_of::<RegFis>() / 4) as u16; // FIS length in DWORDS

        // Setup header flags: length of FIS, direction
        header.flags = (fis_size & 0x1F) | if write { flags::CMD_WRITE } else { 0 };
        header.prdtl = 0; // Will be set to 1 after PRDT is ready
        header.prdbc = 0; // Reset byte count status

        // 4. Access the specific Command Table for this slot
        //    (Calculated offset: Base + Slot * 4K)
        let table_virt =
            VirtAddr::new(self.info.virt_cmd_tables_base.as_u64() + (slot as u64) * PAGE_4K as u64);
        let table = &mut *(table_virt.as_mut_ptr::<HbaCmdTable>());

        // 5. Construct the FIS (the ATA command packet)
        let fis = RegFis::new_dma_command(lba, sector_count, write);
        // Copy FIS into the Command Table
        let fis_ptr = table.cfis.as_mut_ptr() as *mut RegFis;
        write_volatile(fis_ptr, fis);

        // 6. Prepare DMA Buffer (PRDT)
        let total_bytes = sector_count as usize * SECTOR_SIZE;
        if total_bytes == 0 || total_bytes > PAGE_4K {
            // Simplification: We only support 1 PRDT entry (max 4MB ideally, but our allocator is 4K)
            error!("Unsupported transfer size: {} bytes", total_bytes);
            return Err(());
        }

        // ALLOCATE A PINNED DMA PAGE:
        // We use a fixed virtual address range for the DMA buffer for simplicity here,
        // but normally `kalloc_dma_pages` would return a buffer.
        // We map a specific area to ensure we know the physical address for the HBA.
        let dma_page_virt = VirtAddr::new(AHCI_VIRT_BASE + 0x8000 + (slot as u64) * PAGE_4K as u64);
        let dma_page_phys =
            crate::memory::virt_to_phys(dma_page_virt).expect("Failed to map DMA page");

        // Zero buffer for safety
        core::ptr::write_bytes(dma_page_virt.as_mut_ptr::<u8>(), 0, PAGE_4K);

        // If Writing: Copy user data (`buf`) -> DMA buffer (`dma_page_virt`)
        if write {
            core::ptr::copy_nonoverlapping(buf, dma_page_virt.as_mut_ptr::<u8>(), total_bytes);
        }

        // 7. Setup PRDT entry (Physical Region Descriptor)
        let prdt = &mut table.prdt_entry[0];
        prdt.set_data_addr(dma_page_phys); // HBA needs Physical Address
        let dbc_masked = Self::prdt_mask_bytecount((total_bytes as u32).wrapping_sub(1));
        prdt.dbc = dbc_masked | flags::PRDT_IOC; // Interrupt on Completion

        // Finalize header
        header.prdtl = 1; // We used 1 PRDT entry

        // Debug Tracing
        trace!(
            "PRDT: dba={:#x}, dbau={:#x}, dbc={:#x}, total_bytes={}",
            prdt.dba, prdt.dbau, prdt.dbc, total_bytes
        );

        compiler_fence(Ordering::Release); // Ensure memory writes complete before starting hardware

        // 8. Verify Engine is Running
        let cmd_status = self.read_reg(reg::CMD);
        if cmd_status & flags::CMD_ST == 0 {
            error!("Port command engine not running!");
            return Err(());
        }

        // 9. Issue Command (Ring the doorbell)
        self.write_reg(reg::CI, 1 << slot);
        trace!("Command issued on slot {}", slot);

        // 10. Wait for completion
        let res = self.wait_for_completion(slot);

        compiler_fence(Ordering::Acquire); // Ensure we read fresh data after wait

        // 11. If Reading and successful: Copy DMA buffer (`dma_page_virt`) -> user data (`buf`)
        if res.is_ok() && !write {
            core::ptr::copy_nonoverlapping(dma_page_virt.as_ptr::<u8>(), buf, total_bytes);
        }

        // 12. Check for Transaction Errors (Double check TFD)
        let tfd = self.read_reg(reg::TFD);
        if tfd & flags::TFD_ERR != 0 {
            let is = self.read_reg(reg::IS);
            let serr = self.read_reg(reg::SERR);
            error!(
                "AHCI command error: TFD={:#x}, IS={:#x}, SERR={:#x}, slot={}",
                tfd, is, serr, slot
            );
            self.write_reg(reg::IS, is); // Ack error
            self.write_reg(reg::SERR, serr);
            return Err(());
        }

        res
    }

    /// Busy-waits until hardware clears the corresponding bit in CI (Command Issue) register.
    unsafe fn wait_for_completion(&self, slot: u8) -> Result<(), ()> {
        for _ in 0..TIMEOUT_MAX {
            let ci = self.read_reg(reg::CI);
            if ci & (1 << slot) == 0 {
                return Ok(()); // Bit cleared = success
            }

            // Check for immediate errors during wait
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

    /// Wrapper for Read
    unsafe fn read_sectors(&self, lba: u64, buf: *mut u8, count: u16) -> Result<(), ()> {
        self.send_command(lba, buf, count, false)
    }

    /// Wrapper for Write
    unsafe fn write_sectors(&self, lba: u64, buf: *mut u8, count: u16) -> Result<(), ()> {
        self.send_command(lba, buf, count, true)
    }
}

// ============================================================================
// Global State
// ============================================================================

/// Stores PortInfo objects so we can access virtual mapping info later.
static PORTS: Mutex<Option<Vec<PortInfo, 32>>> = Mutex::new(None);

// ============================================================================
// Initialization
// ============================================================================

/// Discover and initialize the AHCI controller and first available port.
///
/// # Procedure
/// 1. Scans PCI bus for AHCI controller.
/// 2. Maps the global ABAR (AHCI Base Address Register).
/// 3. Iterates over implemented ports.
/// 4. For the first active port:
///    - Allocates physical pages for Command List, FIS, and Command Tables.
///    - Maps these pages to fixed virtual addresses.
///    - Configures the Port registers ([`CLB`](Port), [`FB`](Port)) with physical addresses.
///    - Starts the engine.
pub(crate) unsafe fn init() {
    let pci = scan_pci_for_ahci().expect("No AHCI controller found");

    // Map AHCI MMIO region (ABAR)
    let mmio_base = kalloc_page(VirtAddr::new(pci.bar5 as u64), PageType::Mmio)
        .expect("Failed to map AHCI MMIO")
        .as_u64();

    let mut ports = Vec::<_, 32>::new();
    let ports_implemented = read_volatile((mmio_base + reg::PORTS_IMPLEMENTED as u64) as *mut u32);

    for port_num in 0..MAX_SLOTS {
        // Check if port is implemented by controller
        if ports_implemented & (1 << port_num) == 0 {
            continue;
        }

        debug!("Initializing port {}", port_num);

        let port_base =
            (mmio_base + (reg::PORT_BASE + port_num * reg::PORT_SIZE) as u64) as *mut u32;

        // Temporarily create port wrapper
        let port = Port::new(PortInfo {
            port_base: port_base as usize,
            virt_cmd_list: VirtAddr::new(0),
            virt_cmd_tables_base: VirtAddr::new(0),
        });

        // Ensure engine is stopped before configuration
        port.stop();

        // --- Memory Allocation & Mapping ---

        // 1. Assign Virtual Addresses
        let virt_cmd_list = VirtAddr::new(AHCI_VIRT_BASE + port_num as u64 * PAGE_4K as u64);
        let virt_fis = VirtAddr::new(AHCI_VIRT_BASE + 0x1000 + port_num as u64 * PAGE_4K as u64);

        // 2. Allocate Physical Memory & Map to Virtual
        // `kalloc_page` here allocates a physical page and maps it to `virt_addr`.
        let cmd_list_phys = kalloc_page(virt_cmd_list, PageType::Recursive)
            .expect("Failed to allocate command list");
        let fis_phys = kalloc_page(virt_fis, PageType::Recursive).expect("Failed to allocate FIS");

        // 3. Zero Memory (Crucial for stability)
        core::ptr::write_bytes(virt_cmd_list.as_mut_ptr::<u8>(), 0, PAGE_4K);
        core::ptr::write_bytes(virt_fis.as_mut_ptr::<u8>(), 0, PAGE_4K);

        // 4. Write Physical Addresses to Controller Registers
        port.write_reg(reg::CLB, cmd_list_phys.as_u64() as u32);
        port.write_reg(reg::CLBU, (cmd_list_phys.as_u64() >> 32) as u32);
        port.write_reg(reg::FB, fis_phys.as_u64() as u32);
        port.write_reg(reg::FBU, (fis_phys.as_u64() >> 32) as u32);

        // 5. Allocate Command Tables (One per slot)
        let cmd_table_base_virt =
            VirtAddr::new(AHCI_VIRT_BASE + 0x2000 + (port_num * MAX_SLOTS) as u64 * PAGE_4K as u64);

        // Pointer to the Command List we just set up
        let cmd_headers = virt_cmd_list.as_u64() as *mut HbaCmdHeader;

        for slot in 0..MAX_SLOTS {
            let table_virt =
                VirtAddr::new(cmd_table_base_virt.as_u64() + (slot as u64) * PAGE_4K as u64);
            let table_phys = kalloc_page(table_virt, PageType::Recursive)
                .expect("Failed to allocate command table");

            // Zero table
            core::ptr::write_bytes(table_virt.as_mut_ptr::<u8>(), 0, PAGE_4K);

            // Link Command Header -> Command Table Physical Address
            (*cmd_headers.add(slot)).set_command_table_addr(table_phys);
        }

        port.start();

        if port.has_drive() {
            debug!("Drive detected on port {}", port_num);

            // Save the initialized info
            let pinfo = PortInfo {
                port_base: port_base as usize,
                virt_cmd_list,
                virt_cmd_tables_base: cmd_table_base_virt,
            };

            let _ = ports.push(pinfo);

            // Simplification: Stop after first valid drive
            break;
        } else {
            // Cleanup if no drive (optional in simple kernel)
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

/// AHCI-backed implementation of the [`BlockDevice`] trait.
///
/// This struct translates [`Read`]/[`Write`]/[`Seek`] requests into sector-aligned
/// DMA commands.
///
/// # Buffering Logic
/// Hardware only reads/writes full 512-byte sectors. If the user requests a read
/// of 10 bytes at offset 5:
/// 1. We allocate a sector-aligned DMA buffer.
/// 2. We read the full sector into that buffer.
/// 3. We copy bytes 5-15 into the user's buffer.
pub struct AhciBlockDevice {
    port_base: usize,
    sector_size: usize,
    cursor: u64,
    partition_offset: u64, // LBA where partition starts
}

impl BlockDevice for AhciBlockDevice {
    fn block_size(&self) -> u64 {
        self.sector_size as u64
    }
}

impl AhciBlockDevice {
    /// Create a device bound to the given initialized AHCI port.
    pub fn new(port_index: usize) -> Option<Self> {
        let ports = PORTS.lock();
        let ports = ports.as_ref()?;

        ports.get(port_index).map(|pinfo| Self {
            port_base: pinfo.port_base,
            sector_size: SECTOR_SIZE,
            cursor: 0,
            partition_offset: 34, // Hardcoded GPT Data start LBA (TODO: Parse partition table)
        })
    }

    /// Helper to compute how many 512B sectors are needed for `bytes`.
    fn sectors_for_bytes(&self, bytes: usize) -> u16 {
        bytes.div_ceil(self.sector_size) as u16
    }
}

impl IoBase for AhciBlockDevice {
    type Error = FileError;
}

impl Seek for AhciBlockDevice {
    /// Adjust the logical cursor.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.cursor = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(_) => return Err(Self::Error::UnsupportedOperation),
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    self.cursor
                        .checked_sub(offset.unsigned_abs())
                        .ok_or(Self::Error::UnsupportedOperation)?
                } else {
                    self.cursor
                        .checked_add(offset as u64)
                        .ok_or(Self::Error::UnsupportedOperation)?
                }
            }
        };
        Ok(self.cursor)
    }
}

impl Read for AhciBlockDevice {
    /// Read into `buf` starting at the current cursor.
    /// Handles unaligned offsets by reading full sectors and copying relevant slices.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // 1. Calculate alignment
        let start_lba = (self.cursor / SECTOR_SIZE as u64) + self.partition_offset;
        let sector_offset = (self.cursor % SECTOR_SIZE as u64) as usize;

        // 2. Determine size
        let total_bytes_needed = sector_offset + buf.len();
        let sectors = self.sectors_for_bytes(total_bytes_needed);
        let sector_aligned_size = sectors as usize * SECTOR_SIZE;

        // 3. Allocate DMA buffer (Bounce buffer)
        let ubuf = kalloc_dma_pages(sector_aligned_size).map_err(|_| Self::Error::ReadError)?;
        if ubuf.is_empty() {
            return Ok(0);
        }

        // 4. Reconstruct Port wrapper
        let ports = PORTS.lock();
        let pinfo = match ports
            .as_ref()
            .and_then(|v| v.iter().find(|p| p.port_base == self.port_base))
        {
            Some(p) => *p,
            None => return Err(Self::Error::ReadError),
        };
        let port = unsafe { Port::new(pinfo) };

        // 5. Perform Hardware Read
        unsafe {
            port.read_sectors(start_lba, ubuf.as_mut_ptr(), sectors)
                .map_err(|_| Self::Error::ReadError)?;
        }

        // 6. Copy relevant data to User Buffer
        let available_bytes = ubuf.len().saturating_sub(sector_offset);
        let bytes_to_copy = buf.len().min(available_bytes);

        buf[..bytes_to_copy].copy_from_slice(&ubuf[sector_offset..sector_offset + bytes_to_copy]);

        // 7. Update cursor and cleanup
        self.cursor += bytes_to_copy as u64;
        kfree_dma_pages(ubuf).map_err(|_| Self::Error::ReadError)?;
        Ok(bytes_to_copy)
    }
}

impl Write for AhciBlockDevice {
    /// Write `buf` at current cursor.
    /// Performs Read-Modify-Write (RMW) if start/end are not sector-aligned.
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let start_lba = (self.cursor / SECTOR_SIZE as u64) + self.partition_offset;
        let sector_offset = (self.cursor % SECTOR_SIZE as u64) as usize;

        let total_bytes_needed = sector_offset + buf.len();
        let sectors = self.sectors_for_bytes(total_bytes_needed);

        // 1. Allocate DMA buffer
        let ubuf = kalloc_dma_pages(sectors as usize * SECTOR_SIZE)
            .map_err(|_| Self::Error::WriteError)?;

        let ports = PORTS.lock();
        let pinfo = match ports.as_ref().and_then(|v| v.first()) {
            Some(p) => *p,
            None => return Err(Self::Error::WriteError),
        };
        let port = unsafe { Port::new(pinfo) };

        // 2. RMW: If not overwriting the whole block, read existing data first.
        //    (i.e., we are writing to the middle of a sector, or the end of the buffer doesn't align)
        if sector_offset != 0 || !buf.len().is_multiple_of(SECTOR_SIZE) {
            unsafe {
                port.read_sectors(start_lba, ubuf.as_mut_ptr(), sectors)
                    .map_err(|_| Self::Error::WriteError)?;
            }
        }

        // 3. Overlay new data onto the buffer
        ubuf[sector_offset..sector_offset + buf.len()].copy_from_slice(buf);

        // 4. Perform Hardware Write
        unsafe {
            port.write_sectors(start_lba, ubuf.as_mut_ptr(), sectors)
                .map_err(|_| Self::Error::WriteError)?;
        }

        self.cursor += buf.len() as u64;
        kfree_dma_pages(ubuf).map_err(|_| Self::Error::WriteError)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // For now, we are synchronous, so flush is a no-op.
        Ok(())
    }
}
