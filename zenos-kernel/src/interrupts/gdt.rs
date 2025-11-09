use crate::memory::{HIGHER_HALF_BASE, PageType, kalloc_page};
use spin::Lazy;
use x86_64::{
    VirtAddr,
    instructions::segmentation::Segment,
    instructions::tables::load_tss,
    registers::segmentation::CS,
    structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
    structures::tss::TaskStateSegment,
};

pub(super) const DOUBLE_FAULT_IST_INDEX: usize = 0;
pub static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();

    // Allocate and map a page for the double fault stack
    let stack_phys = kalloc_page(VirtAddr::new(0xFFFF_FF00_0000_0000), PageType::Arbitrary)
        .expect("Failed to allocate stack for double fault IST");

    let stack_virt = VirtAddr::new(HIGHER_HALF_BASE + stack_phys.as_u64());

    // Set the stack pointer to the *end* of the page (because the stack grows down)
    let stack_top = stack_virt + 4096;

    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX] = stack_top;
    tss.privilege_stack_table[0] = VirtAddr::new(crate::memory::KERNEL_STACK_BASE);
    tss
});

pub static GDT: Lazy<GdtWrapper> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(Descriptor::kernel_code_segment());
    let data_segment = gdt.append(Descriptor::kernel_data_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    let user_code_segment = gdt.append(Descriptor::user_code_segment());
    let user_data_segment = gdt.append(Descriptor::user_data_segment());
    GdtWrapper::new((
        gdt,
        code_selector,
        tss_selector,
        data_segment,
        user_code_segment,
        user_data_segment,
    ))
});

#[derive(Debug)]
pub struct GdtWrapper {
    pub gdt: GlobalDescriptorTable,
    pub code_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
    pub _data_selector: SegmentSelector,
    pub user_code_segment: SegmentSelector,
    pub user_data_segment: SegmentSelector,
}

impl GdtWrapper {
    pub fn new(
        slop: (
            GlobalDescriptorTable,
            SegmentSelector,
            SegmentSelector,
            SegmentSelector,
            SegmentSelector,
            SegmentSelector,
        ),
    ) -> Self {
        Self {
            gdt: slop.0,
            code_selector: slop.1,
            tss_selector: slop.2,
            _data_selector: slop.3,
            user_code_segment: slop.4,
            user_data_segment: slop.5,
        }
    }

    pub fn load(&'static self) {
        self.gdt.load();
    }
}

pub fn init_gdt() {
    GDT.load();
    unsafe {
        CS::set_reg(GDT.code_selector);
        load_tss(GDT.tss_selector);
    }
}
