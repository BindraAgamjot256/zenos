use crate::memory::{
    KERNEL_BASE, KERNEL_CR3_SCRATCH, PageType, active_level_4_table, kalloc_page, kleak_page,
};
use x86_64::structures::paging::PageTable;
use x86_64::{PhysAddr, VirtAddr};

pub(crate) unsafe fn create_cr3_from_current_page_tables() -> PhysAddr {
    let phys =
        kalloc_page(VirtAddr::new(KERNEL_CR3_SCRATCH), PageType::Arbitrary).expect("map failed");
    let active_table = active_level_4_table(VirtAddr::new(KERNEL_BASE));
    let new_table_ptr = KERNEL_CR3_SCRATCH as *mut PageTable;
    let new_table: &mut PageTable = &mut *new_table_ptr;

    // 4. Zero the new table to prevent random crashes
    new_table.zero();

    // 5. COPY KERNEL MAPPINGS
    // In x86_64, the address space is split in half.
    // 0..256 = User Space (We leave this empty for now)
    // 256..512 = Kernel Space (We must copy this)
    for i in 256..512 {
        // We copy the entry from the active table to the new one.
        // This copies the flags (PRESENT, GLOBAL, WRITABLE) and the physical frame.
        new_table[i] = active_table[i].clone();
    }

    kleak_page(VirtAddr::new(KERNEL_CR3_SCRATCH), PageType::Arbitrary).expect("unmap failed");

    phys
}
