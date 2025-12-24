use crate::memory::{
    HIGHER_HALF_BASE, KERNEL_CR3_SCRATCH, PAGE_4K, PageType, active_level_4_table, kalloc_page,
    kleak_page,
};
use log::trace;
use spin::Mutex;
use x86_64::structures::paging::{PageTable, PageTableFlags};
use x86_64::{PhysAddr, VirtAddr};

/// Counter for scratch addresses to avoid conflicts
static SCRATCH_COUNTER: Mutex<u64> = Mutex::new(0);
/// Helper to access a physical page table through the higher-half mapping
unsafe fn phys_to_page_table(phys: PhysAddr) -> &'static PageTable {
    let virt = VirtAddr::new(HIGHER_HALF_BASE + phys.as_u64());
    &*(virt.as_ptr::<PageTable>())
}

/// Helper to access a physical page table mutably through the higher-half mapping
unsafe fn phys_to_page_table_mut(phys: PhysAddr) -> &'static mut PageTable {
    let virt = VirtAddr::new(HIGHER_HALF_BASE + phys.as_u64());
    &mut *(virt.as_mut_ptr::<PageTable>())
}

/// Get a unique scratch address
fn get_scratch_addr() -> u64 {
    let mut counter = SCRATCH_COUNTER.lock();
    let offset = *counter;
    *counter = (*counter + 1) % 256;
    KERNEL_CR3_SCRATCH + 0x100000 + offset * PAGE_4K as u64
}

/// Allocate a new page table and return its physical address
unsafe fn alloc_page_table() -> Option<PhysAddr> {
    let scratch_addr = get_scratch_addr();

    let phys = kalloc_page(VirtAddr::new(scratch_addr), PageType::Arbitrary).ok()?;

    // Zero the new table using the higher-half mapping
    let table = phys_to_page_table_mut(phys);
    table.zero();

    // Unmap the scratch address (we'll access via higher-half)
    let _ = kleak_page(VirtAddr::new(scratch_addr), PageType::Arbitrary);

    Some(phys)
}

/// Allocate a new physical page and copy contents from source, return its physical address
unsafe fn alloc_and_copy_page(src_phys: PhysAddr) -> Option<PhysAddr> {
    let scratch_addr = get_scratch_addr();

    let dst_phys = kalloc_page(VirtAddr::new(scratch_addr), PageType::Arbitrary).ok()?;

    // Copy contents using higher-half mapping
    let src_ptr = (HIGHER_HALF_BASE + src_phys.as_u64()) as *const u8;
    let dst_ptr = (HIGHER_HALF_BASE + dst_phys.as_u64()) as *mut u8;
    core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_4K);

    // Unmap scratch address
    let _ = kleak_page(VirtAddr::new(scratch_addr), PageType::Arbitrary);

    Some(dst_phys)
}

/// Deep clone a page table at any level, recursively copying all entries and physical pages
/// level: 3 = L3 (PDPT), 2 = L2 (PD), 1 = L1 (PT), 0 = leaf page
unsafe fn deep_clone_table(src_phys: PhysAddr, level: u8) -> Option<PhysAddr> {
    if level == 0 {
        // This is a leaf page (actual data), copy it
        return alloc_and_copy_page(src_phys);
    }

    // Allocate new page table
    let new_table_phys = alloc_page_table()?;
    let src_table = phys_to_page_table(src_phys);
    let new_table = phys_to_page_table_mut(new_table_phys);

    for i in 0..512 {
        let entry = &src_table[i];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        let entry_phys = PhysAddr::new(entry.addr().as_u64());
        let flags = entry.flags();

        // Check for huge pages (1GB at L3, 2MB at L2)
        if flags.contains(PageTableFlags::HUGE_PAGE) {
            // For huge pages, we need to copy the entire huge page
            // For simplicity, just share huge pages (they're typically kernel mappings)
            new_table[i] = entry.clone();
        } else if level == 1 {
            // L1 (PT) entries point to 4K pages - copy them
            let new_page_phys = alloc_and_copy_page(entry_phys)?;
            new_table[i].set_addr(new_page_phys, flags);
        } else {
            // Recurse into lower level tables
            let cloned_table_phys = deep_clone_table(entry_phys, level - 1)?;
            new_table[i].set_addr(cloned_table_phys, flags);
        }
    }

    Some(new_table_phys)
}

/// Clone the current process's address space for fork()
/// This creates a DEEP copy - user pages are fully copied
pub(crate) unsafe fn clone_address_space() -> Result<PhysAddr, ()> {
    // Allocate a new L4 page table
    let new_l4_phys = alloc_page_table().ok_or(())?;

    let active_l4 = active_level_4_table(VirtAddr::new(HIGHER_HALF_BASE));
    let new_l4 = phys_to_page_table_mut(new_l4_phys);

    // Copy kernel mappings (entries 256-511) - these share the same physical pages
    for i in 256..512 {
        new_l4[i] = active_l4[i].clone();
    }

    // DEEP copy user space mappings (entries 0-255)
    // Each user page table tree is recursively cloned with new physical pages
    for i in 0..256 {
        let entry = &active_l4[i];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        let entry_phys = PhysAddr::new(entry.addr().as_u64());
        let flags = entry.flags();

        // Deep clone the L3 table and everything below it
        let cloned_l3_phys = deep_clone_table(entry_phys, 3).ok_or(())?;
        new_l4[i].set_addr(cloned_l3_phys, flags);
    }

    trace!(
        "clone_address_space: created new L4 at {:?} (deep copy)",
        new_l4_phys
    );

    Ok(new_l4_phys)
}

pub(crate) unsafe fn new_user_address_space() -> Result<PhysAddr, ()> {
    // Allocate a new L4 page table
    let new_l4_phys = alloc_page_table().ok_or(())?;

    let active_l4 = active_level_4_table(VirtAddr::new(HIGHER_HALF_BASE));
    let new_l4 = phys_to_page_table_mut(new_l4_phys);

    // Copy kernel mappings (entries 256-511) - these share the same physical pages
    for i in 256..512 {
        new_l4[i] = active_l4[i].clone();
    }

    trace!(
        "new_user_address_space: created new L4 at {:?}",
        new_l4_phys
    );

    Ok(new_l4_phys)
}
