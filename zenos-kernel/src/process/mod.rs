use crate::fs::FS;
use crate::interrupts::gdt::GDT;
use crate::kprintln;
use crate::memory::{PAGE_4K, PageType, kalloc_page, ualloc_page, ualloc_page_flags};
use crate::percpu::{PerCpuData, PerCpuVar};
use crate::testing::Testable;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;
use core::mem::offset_of;
use core::sync::atomic::{AtomicU64, Ordering};
use fatfs::{Read, Seek, SeekFrom};
use log::{error, info, trace};
use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::structures::paging::PageTableFlags;
use xmas_elf::header::Type as ElfType;
use xmas_elf::program;
use xmas_elf::program::Type as PhType;

// Choose a default userspace base for PIE/ET_DYN binaries
const DEFAULT_USER_BASE: u64 = 0x0000_0000_0040_0000; // 4 MiB, away from null page
const ELF_ADDR: u64 = 0x1000000;

#[derive(Debug)]
pub struct Process<'a> {
    pub pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    code: xmas_elf::ElfFile<'a>,
    // Base address to load the binary at (0 for ET_EXEC, DEFAULT_USER_BASE for ET_DYN/PIE)
    load_bias: u64,
}

impl<'a> Process<'a> {
    fn new(parent: &Process, name: String, code: &'a [u8]) -> Self {
        let pid = NEXT_PID.load(Ordering::Acquire);
        let elf = xmas_elf::ElfFile::new(code).expect("Failed to parse process");
        let load_bias = compute_load_bias(&elf);
        let p = Process {
            pid,
            parent_pid: parent.pid,
            state: ProcessState::new(),
            name,
            code: elf,
            load_bias,
        };
        NEXT_PID.store(pid + 1, Ordering::Release);
        p
    }

    pub fn load(&mut self) {
        let elf = &self.code;
        let header = elf.header;
        info!("ELF header: {:?}", header);
        self.state.loaded = true;

        for program_header in elf.program_iter() {
            let res = program::sanity_check(program_header, &elf);
            if res.is_err() {
                error!("Invalid program header: {:?}", res.err().unwrap());
                panic!("Invalid program header");
            } else {
                continue;
            }
        }

        for program_header in elf.program_iter() {
            match program_header.get_type() {
                Ok(PhType::Load) => {
                    let segment = program_header;
                    let start_addr = segment.virtual_addr();
                    let end_addr = segment.virtual_addr() + segment.file_size();
                    let flags = segment.flags();
                    info!(
                        "Loading segment: {:#?}, start-end = {start_addr:x}-{end_addr:x}",
                        segment
                    );
                    let mut ptf = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
                    if flags.is_write() {
                        ptf |= PageTableFlags::WRITABLE;
                    }
                    if !flags.is_execute() {
                        ptf |= PageTableFlags::NO_EXECUTE;
                    }
                    // Temporarily disable W^X while bringing up userland
                    ptf |= PageTableFlags::WRITABLE;
                    let mem_size = segment.mem_size();
                    let seg_start = VirtAddr::new(segment.virtual_addr() + self.load_bias);
                    let seg_end = VirtAddr::new(segment.virtual_addr() + mem_size + self.load_bias);
                    let page_start = seg_start.align_down(PAGE_4K as u64);
                    let page_end = seg_end.align_up(PAGE_4K as u64);

                    let mut addr = page_start;
                    while addr < page_end {
                        ualloc_page_flags(addr, PageType::Arbitrary, ptf).unwrap();
                        addr += PAGE_4K as u64;
                    }
                    let src = (ELF_ADDR + segment.offset()) as *mut u8;
                    let dst =
                        ((segment.virtual_addr() as *mut u8) as u64 + self.load_bias) as *mut u8;
                    let len = segment.file_size() as usize;

                    info!("Copy: src={:#x}, dst={:#x}, len={:#x}", src as u64, dst as u64, len);
                    
                    // Check if entry point will be in this segment
                    let entry = self.code.header.pt2.entry_point();
                    if entry >= segment.virtual_addr() && entry < segment.virtual_addr() + segment.file_size() {
                        let offset_in_seg = (entry - segment.virtual_addr()) as usize;
                        let src_entry = unsafe { (src as *const u8).add(offset_in_seg) };
                        let src_entry_bytes = unsafe { core::slice::from_raw_parts(src_entry, 16) };
                        info!("Entry {:#x} is in this segment at offset {:#x}, source bytes: {:x?}", entry, offset_in_seg, src_entry_bytes);
                    }

                    unsafe {
                        core::ptr::copy(src, dst, len);
                    }
                    info!(
                        "Segment {} copied to {:#x}-{:#x}",
                        segment.virtual_addr(),
                        seg_start.as_u64(),
                        seg_end.as_u64()
                    );
                    // Verify the copy
                    let copied_bytes = unsafe { core::slice::from_raw_parts(dst, 16.min(len)) };
                    info!("First bytes copied: {:x?}", copied_bytes);
                    let bss_len = segment.mem_size() - segment.file_size();
                    if bss_len > 0 {
                        unsafe {
                            core::ptr::write_bytes(
                                (segment.virtual_addr() + segment.file_size() + self.load_bias)
                                    as *mut u8,
                                0,
                                bss_len as usize,
                            );
                        }
                    }
                }
                _ => {
                    info!("Skipping segment: {:?}", program_header);
                }
            }
        }
    }

    pub fn prepare_run(&mut self) -> Result<(u64, u64), ()> {
        if !self.state.loaded {
            return Err(());
        }
        unsafe {
            CURRENT_PID.write(self.pid);
        }

        // Determine a safe user stack top: above the highest mapped segment + some gap
        let mut max_end = 0u64;
        for ph in self.code.program_iter() {
            if let Ok(PhType::Load) = ph.get_type() {
                let end = ph.virtual_addr() + ph.mem_size();
                if end > max_end {
                    max_end = end;
                }
            }
        }
        let stack_gap = 0x20_000; // 128 KiB gap above image
        let stack_size = 0x4000; // 16 KiB user stack
        let user_stack_top =
            (self.load_bias + max_end + stack_gap + (PAGE_4K as u64 - 1)) & !((PAGE_4K as u64) - 1);
        
        info!("Allocating user stack: max_end={:#x}, stack_top={:#x}, stack_size={:#x}", 
              max_end, user_stack_top, stack_size);
        
        let stack_start = user_stack_top - stack_size;
        let mut addr = stack_start;
        while addr < user_stack_top {
            info!("Allocating stack page at {:#x}", addr);
            ualloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
            addr += PAGE_4K as u64;
        }

        self.state.rip = self.code.header.pt2.entry_point() + self.load_bias;
        let user_stack = user_stack_top;
        let user_entry = self.state.rip;

        info!("Prepared process: {}", self.name);
        Ok((user_entry, user_stack))
    }

    pub fn trace(&self) {
        kprintln!("{:#?}", self.state);
    }
}

pub fn enter_user_mode(user_entry: u64, user_stack: u64) -> ! {
    let user_cs = (GDT.user_code_segment.0 | 3) as u64;
    let user_ss = (GDT.user_data_segment.0 | 3) as u64;
    let rflags = 0x202u64; // IF = 1

    info!("Switching to user mode");
    info!("Entry: {user_entry:x}");
    info!("Stack: {user_stack:x}");
    info!("GDT user_code raw: {:#x}, user_data raw: {:#x}", GDT.user_code_segment.0, GDT.user_data_segment.0);
    info!("CS: {user_cs:x}");
    info!("SS: {user_ss:x}");
    info!("RFLAGS: {rflags:x}");
    
    // Sanity check: verify we can read the entry point
    let entry_bytes = unsafe { core::slice::from_raw_parts(user_entry as *const u8, 16) };
    info!("Entry point bytes: {:x?}", entry_bytes);
    
    assert_eq!(user_stack % 16, 0, "stack must be 16-byte aligned");

    // Build a user IRET frame and drop to ring 3
    unsafe {
        asm!(
            "cli",                    // be explicit; IF will be restored from RFLAGS
            "push {user_ss}",         // SS
            "push {user_rsp}",        // RSP
            "push {rflags}",          // RFLAGS
            "push {user_cs}",         // CS
            "push {user_rip}",        // RIP
            "iretq",
            user_ss = in(reg) user_ss,
            user_rsp = in(reg) user_stack,
            rflags = in(reg) rflags,
            user_cs = in(reg) user_cs,
            user_rip = in(reg) user_entry,
            options(noreturn)
        );
    }
}

pub fn switch_to(process: &Process) {
    let mut p = PROCESSES.lock();
    let mut p_idx = None;
    for (i, p_ref) in p.iter().enumerate() {
        if p_ref.pid == process.pid {
            p_idx = Some(i);
            break;
        }
    }
    // TODO: This needs to be updated to use prepare_run and enter_user_mode
    // But switch_to seems unused for now or at least not in the main path I'm fixing.
    // I'll leave it broken/commented or fix it if I can.
    // Since I can't easily change the signature of switch_to to drop the lock inside,
    // and it takes &Process which might be from the lock...
    // Actually switch_to takes &Process, then locks PROCESSES again? That would deadlock if called with a reference from the lock.
    // But here it locks PROCESSES.
    
    if let Some(i) = p_idx {
        let proc = &mut p[i];
        // proc.run().unwrap(); // run() is gone
        let (entry, stack) = proc.prepare_run().unwrap();
        drop(p); // Release lock!
        enter_user_mode(entry, stack);
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct ProcessState {
    rax: u64,
    rip: u64,
    cr3: u64,
    loaded: bool,
}

impl ProcessState {
    fn new() -> Self {
        ProcessState {
            rax: 0,
            rip: 0,
            cr3: 0,
            loaded: false,
        }
    }
}

pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init_process() {
    let fs = FS.lock();
    let mut file = match fs
        .root_dir()
        .open_dir("bin")
        .and_then(|b| b.open_file("init.elf"))
    {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open init: {:?}", e);
            error!("are you sure you are using a proper build?");
            panic!("Failed to open init");
        }
    };
    let len = get_len(&mut file).unwrap_or(0);
    let pages = (len + PAGE_4K as u64) / PAGE_4K as u64;
    for page in 0..pages {
        kalloc_page(
            VirtAddr::new(ELF_ADDR + (page * PAGE_4K as u64)),
            PageType::Arbitrary,
        )
        .expect("Map failure");
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(ELF_ADDR as *mut u8, len as usize) };
    trace!("len: {} bytes", len);
    
    let mut offset = 0;
    loop {
        match file.read(&mut buf[offset..]) {
            Ok(0) => break,
            Ok(n) => offset += n,
            Err(e) => {
                error!("Failed to read init: {:?}", e);
                break;
            }
        }
        if offset >= len as usize {
            break;
        }
    }
    info!("Read {} bytes from init.elf", offset);

    trace!("init code copied to 0x{:x}", ELF_ADDR);
    let elf = xmas_elf::ElfFile::new(buf).expect("Failed to parse init");
    let load_bias = compute_load_bias(&elf);
    let process = Process {
        pid: 0,
        parent_pid: u64::MAX,
        state: ProcessState::default(),
        name: String::from("init"),
        code: elf,
        load_bias,
    };
    PROCESSES.lock().push(process);
}

fn get_len<T: Seek>(obj: &mut T) -> Result<u64, ()> {
    let current_pos = obj.seek(SeekFrom::Current(0)).map_err(|e| {
        error!("Failed to get stream position: {:?}", e);
    })?;
    let end = obj
        .seek(SeekFrom::End(0))
        .map_err(|e| error!("Seek failed: {:?}", e))?;
    obj.seek(SeekFrom::Start(current_pos))
        .map_err(|e| error!("Seek restore failed: {:?}", e))?;
    Ok(end)
}
static CURRENT_PID: PerCpuVar<u64> = PerCpuVar::new(offset_of!(PerCpuData, curr_pid));

pub(crate) static TESTS: &[&(dyn Testable + Sync)] = {
    if cfg!(test) || cfg!(debug_assertions) {
        &[
            &tests::test_get_len_empty,
            &tests::test_get_len_large_file,
            &tests::test_get_len_preserves_position,
            &tests::test_get_len_with_data,
            &tests::test_process_state_copy,
            &tests::test_process_state_default,
            &tests::test_process_state_new,
        ]
    } else {
        &[]
    }
};

// Helper to choose a per-process load bias for PIC/PIE binaries
fn compute_load_bias(elf: &xmas_elf::ElfFile) -> u64 {
    match elf.header.pt2.type_().as_type() {
        ElfType::SharedObject => DEFAULT_USER_BASE,
        _ => 0,
    }
}

mod tests {
    use super::*;
    use crate::test_assert_eq as assert_eq;
    use fatfs::IoBase;

    pub fn test_process_state_new() -> Option<()> {
        let state = ProcessState::new();
        assert_eq!(state.rax, 0);
        assert_eq!(state.rip, 0);
        assert_eq!(state.cr3, 0);
        assert_eq!(state.loaded, false);
        Some(())
    }

    pub fn test_process_state_default() -> Option<()> {
        let state = ProcessState::default();
        assert_eq!(state.rax, 0);
        assert_eq!(state.rip, 0);
        assert_eq!(state.cr3, 0);
        assert_eq!(state.loaded, false);
        Some(())
    }

    pub fn test_process_state_copy() -> Option<()> {
        let state1 = ProcessState {
            rax: 42,
            rip: 0x1000,
            cr3: 0x2000,
            loaded: true,
        };
        let state2 = state1;
        assert_eq!(state1.rax, state2.rax);
        assert_eq!(state1.rip, state2.rip);
        assert_eq!(state1.cr3, state2.cr3);
        assert_eq!(state1.loaded, state2.loaded);
        Some(())
    }

    // Mock seekable object for testing get_len
    struct MockSeekable {
        pos: u64,
        len: u64,
    }

    impl MockSeekable {
        fn new(len: u64) -> Self {
            MockSeekable { pos: 0, len }
        }
    }

    impl IoBase for MockSeekable {
        type Error = ();
    }

    impl Seek for MockSeekable {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64, ()> {
            match pos {
                SeekFrom::Start(n) => {
                    self.pos = n;
                    Ok(self.pos)
                }
                SeekFrom::Current(n) => {
                    self.pos = (self.pos as i64 + n) as u64;
                    Ok(self.pos)
                }
                SeekFrom::End(n) => {
                    self.pos = (self.len as i64 + n) as u64;
                    Ok(self.pos)
                }
            }
        }
    }

    pub fn test_get_len_empty() -> Option<()> {
        let mut obj = MockSeekable::new(0);
        let len = get_len(&mut obj);
        assert_eq!(len, Ok(0));
        assert_eq!(obj.pos, 0); // Position should be restored
        Some(())
    }

    pub fn test_get_len_with_data() -> Option<()> {
        let mut obj = MockSeekable::new(1024);
        let len = get_len(&mut obj);
        assert_eq!(len, Ok(1024));
        assert_eq!(obj.pos, 0); // Position should be restored
        Some(())
    }

    pub fn test_get_len_preserves_position() -> Option<()> {
        let mut obj = MockSeekable::new(1024);
        obj.seek(SeekFrom::Start(512)).unwrap();
        let len = get_len(&mut obj);
        assert_eq!(len, Ok(1024));
        assert_eq!(obj.pos, 512); // Position should be restored to 512
        Some(())
    }

    pub fn test_get_len_large_file() -> Option<()> {
        let mut obj = MockSeekable::new(10 * 1024 * 1024); // 10 MB
        let len = get_len(&mut obj);
        assert_eq!(len, Ok(10 * 1024 * 1024));
        assert_eq!(obj.pos, 0);
        Some(())
    }
}
