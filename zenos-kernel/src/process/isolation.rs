use crate::memory::{
    HIGHER_HALF_BASE, KERNEL_CR3_SCRATCH, PAGE_4K, PageType, active_level_4_table, kalloc_page,
    kleak_page,
};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use hashbrown::HashMap;
use log::{error, trace};
use spin::{Lazy, Mutex};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame};
use x86_64::{PhysAddr, VirtAddr};

/// Software-defined flag for COW pages (using bit 9, available for OS use)
pub const COW_FLAG: PageTableFlags = PageTableFlags::BIT_9;

/// Reference count tracking for physical pages used in COW
static PAGE_REFCOUNTS: Lazy<Mutex<HashMap<u64, AtomicU32>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Increment reference count for a physical page
fn refcount_inc(phys: PhysAddr) {
    let mut refcounts = PAGE_REFCOUNTS.lock();
    let key = phys.as_u64();
    if let Some(count) = refcounts.get(&key) {
        count.fetch_add(1, Ordering::AcqRel);
    } else {
        refcounts.insert(key, AtomicU32::new(2)); // Parent + child both reference it
    }
}

/// Decrement reference count for a physical page, returns true if this was the last reference
pub fn refcount_dec(phys: PhysAddr) -> bool {
    let mut refcounts = PAGE_REFCOUNTS.lock();
    let key = phys.as_u64();
    if let Some(count) = refcounts.get(&key) {
        let old = count.fetch_sub(1, Ordering::AcqRel);
        if old == 1 {
            refcounts.remove(&key);
            return true;
        }
    }
    false
}

/// Get reference count for a physical page (1 means exclusive ownership)
pub fn refcount_get(phys: PhysAddr) -> u32 {
    let refcounts = PAGE_REFCOUNTS.lock();
    refcounts
        .get(&phys.as_u64())
        .map(|c| c.load(Ordering::Acquire))
        .unwrap_or(1) // Not tracked means exclusive ownership
}

/// Counter for scratch addresses to avoid conflicts
static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Helper to access a physical page table mutably through the higher-half mapping
unsafe fn phys_to_page_table_mut(phys: PhysAddr) -> &'static mut PageTable {
    let virt = VirtAddr::new(HIGHER_HALF_BASE + phys.as_u64());
    &mut *(virt.as_mut_ptr::<PageTable>())
}

/// Get a unique scratch address
fn get_scratch_addr() -> u64 {
    let offset = SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed) % 256;
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

/// Clone a page table using copy-on-write for leaf pages
/// level: 3 = L3 (PDPT), 2 = L2 (PD), 1 = L1 (PT)
/// For level 1 (PT), writable pages are marked read-only with COW flag in both parent and child
unsafe fn cow_clone_table(src_phys: PhysAddr, level: u8) -> Option<PhysAddr> {
    // Allocate new page table
    let new_table_phys = alloc_page_table()?;
    let src_table = phys_to_page_table_mut(src_phys);
    let new_table = phys_to_page_table_mut(new_table_phys);

    for i in 0..512 {
        let entry = &mut src_table[i];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        let entry_phys = PhysAddr::new(entry.addr().as_u64());
        let flags = entry.flags();

        // Check for huge pages (1GB at L3, 2MB at L2)
        if flags.contains(PageTableFlags::HUGE_PAGE) {
            // For simplicity, just share huge pages (they're typically kernel mappings)
            new_table[i] = entry.clone();
        } else if level == 1 {
            // L1 (PT) entries point to 4K pages - use COW
            if flags.contains(PageTableFlags::WRITABLE) {
                // Mark parent page as read-only + COW
                let cow_flags = (flags - PageTableFlags::WRITABLE) | COW_FLAG;
                entry.set_addr(entry_phys, cow_flags);

                // Child gets same physical page, also read-only + COW
                new_table[i].set_addr(entry_phys, cow_flags);

                // Track reference count
                refcount_inc(entry_phys);
            } else {
                // Read-only page, just share it directly
                new_table[i] = entry.clone();
            }
        } else {
            // Recurse into lower level tables
            let cloned_table_phys = cow_clone_table(entry_phys, level - 1)?;
            new_table[i].set_addr(cloned_table_phys, flags);
        }
    }

    Some(new_table_phys)
}

/// Clone the current process's address space for fork() using copy-on-write
/// Writable user pages are shared read-only and copied on first write
pub(crate) unsafe fn clone_address_space() -> Result<PhysAddr, ()> {
    // Allocate a new L4 page table
    let new_l4_phys = alloc_page_table().ok_or(())?;

    let active_l4 = active_level_4_table(VirtAddr::new(HIGHER_HALF_BASE));
    let new_l4 = phys_to_page_table_mut(new_l4_phys);

    // Copy kernel mappings (entries 256-511) - these share the same physical pages
    for i in 256..512 {
        new_l4[i] = active_l4[i].clone();
    }

    // COW clone user space mappings (entries 0-255)
    // Writable pages are marked read-only with COW flag in both parent and child
    for i in 0..256 {
        let entry = &active_l4[i];
        if !entry.flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        let entry_phys = PhysAddr::new(entry.addr().as_u64());
        let flags = entry.flags();

        // COW clone the L3 table and everything below it
        let cloned_l3_phys = cow_clone_table(entry_phys, 3).ok_or(())?;
        new_l4[i].set_addr(cloned_l3_phys, flags);
    }

    new_l4[511].set_frame(
        PhysFrame::containing_address(new_l4_phys),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );

    // Flush TLB since we modified parent's page table entries
    x86_64::instructions::tlb::flush_all();

    trace!(
        "clone_address_space: created new L4 at {:?} (COW)",
        new_l4_phys
    );

    Ok(new_l4_phys)
}

/// Handle a copy-on-write page fault
/// Returns true if the fault was handled (was a COW page), false otherwise
pub unsafe fn handle_cow_fault(fault_addr: VirtAddr) -> bool {
    let p4 = active_level_4_table(VirtAddr::new(HIGHER_HALF_BASE));

    let p4_idx = fault_addr.p4_index();
    let p3_idx = fault_addr.p3_index();
    let p2_idx = fault_addr.p2_index();
    let p1_idx = fault_addr.p1_index();

    // Walk page tables to find the PTE
    let p4e = &p4[p4_idx];
    if !p4e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }

    let p3 = phys_to_page_table_mut(p4e.addr());
    let p3e = &p3[p3_idx];
    if !p3e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }

    let p2 = phys_to_page_table_mut(p3e.addr());
    let p2e = &p2[p2_idx];
    if !p2e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }

    let p1 = phys_to_page_table_mut(p2e.addr());
    let p1e = &mut p1[p1_idx];
    if !p1e.flags().contains(PageTableFlags::PRESENT) {
        return false;
    }

    // Check if this is a COW page
    if !p1e.flags().contains(COW_FLAG) {
        return false; // Not a COW page, nothing to do
    }

    let old_phys = p1e.addr();
    let old_flags = p1e.flags();

    // Check refcount to decide whether to copy or just make writable
    let refcount = refcount_get(old_phys);

    if refcount == 1 {
        // We're the only owner, just make it writable again
        let new_flags = (old_flags - COW_FLAG) | PageTableFlags::WRITABLE;
        p1e.set_addr(old_phys, new_flags);
    } else {
        // Multiple owners, need to copy
        let new_phys = alloc_and_copy_page(old_phys);
        if new_phys.is_none() {
            return false; // Out of memory
        }
        let new_phys = new_phys.unwrap();

        // Update PTE to point to new page with write permission
        let new_flags = (old_flags - COW_FLAG) | PageTableFlags::WRITABLE;
        p1e.set_addr(new_phys, new_flags);

        // Decrement refcount on old page
        refcount_dec(old_phys);
    }

    // Flush TLB for this address
    x86_64::instructions::tlb::flush(fault_addr);

    trace!("COW fault handled for {:?}", fault_addr);
    true
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
