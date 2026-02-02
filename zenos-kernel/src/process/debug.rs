use crate::serial_println as println;
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{PageTable, PageTableFlags},
};

/// Dump page table entries for a virtual address
/// Requires recursive mapping to be active
/// this function was only made so i could figure out why the fuck processes segfault 25% of the time. do not take it seriously.
pub fn dump_pte(addr: VirtAddr) {
    let (level_4_frame, _) = Cr3::read();
    let p4_phys = level_4_frame.start_address();
    let p4_virt = recursive_map(p4_phys);

    let p4: &PageTable = unsafe { &*p4_virt.as_ptr() };

    let p4_index = addr.p4_index();
    let p3_index = addr.p3_index();
    let p2_index = addr.p2_index();
    let p1_index = addr.p1_index();

    let p4e = &p4[p4_index];
    println!("P4E @ {:?} -> {:?}", p4e.addr(), p4e.flags());

    if !p4e.flags().contains(PageTableFlags::PRESENT) {
        println!("P4E not present, stop coping");
        return;
    }

    let p3_virt = recursive_map(p4e.addr());
    let p3: &PageTable = unsafe { &*p3_virt.as_ptr() };
    let p3e = &p3[p3_index];
    println!("P3E @ {:?} -> {:?}", p3e.addr(), p3e.flags());

    if !p3e.flags().contains(PageTableFlags::PRESENT) {
        println!("P3E not present, dreams crushed");
        return;
    }

    if p3e.flags().contains(PageTableFlags::HUGE_PAGE) {
        println!("1GiB huge page, permissions live here");
        return;
    }

    let p2_virt = recursive_map(p3e.addr());
    let p2: &PageTable = unsafe { &*p2_virt.as_ptr() };
    let p2e = &p2[p2_index];
    println!("P2E @ {:?} -> {:?}", p2e.addr(), p2e.flags());

    if !p2e.flags().contains(PageTableFlags::PRESENT) {
        println!("P2E not present, why are you like this");
        return;
    }

    if p2e.flags().contains(PageTableFlags::HUGE_PAGE) {
        println!("2MiB huge page, permissions live here");
        return;
    }

    let p1_virt = recursive_map(p2e.addr());
    let p1: &PageTable = unsafe { &*p1_virt.as_ptr() };
    let p1e = &p1[p1_index];
    println!("P1E @ {:?} -> {:?}", p1e.addr(), p1e.flags());

    if !p1e.flags().contains(PageTableFlags::PRESENT) {
        println!("P1E not present, memory said no");
        return;
    }

    println!("Final mapping OK, flags above are the truth");
}

/// Convert a physical address to a virtual one via recursive mapping
/// Assumes P4 is recursively mapped at 0xffff_ffff_ffff_f000
fn recursive_map(phys: PhysAddr) -> VirtAddr {
    let phys_u64 = phys.as_u64();
    let virt = 0xffff_ffff_ffff_f000u64 | phys_u64;
    VirtAddr::new(virt)
}
