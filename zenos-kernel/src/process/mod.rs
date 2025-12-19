pub(crate) mod file_handles;
mod isolation;
mod scheduler;

use crate::disk::vfs::{File, SeekFrom};
use crate::disk::FileError;
use crate::disk::FS;
use crate::percpu::PerCpuVar;
use crate::process::file_handles::{FileHandle, FileOpenOptions, Stderr, Stdin, Stdout};
use crate::process::isolation::create_cr3_from_current_page_tables;
use crate::process::scheduler::Scheduler;
use crate::{
    interrupts::gdt::GDT,
    kprintln,
    memory::{kalloc_page, ualloc_page, ualloc_page_flags, PageType, KERNEL_BASE, PAGE_4K},
    percpu::PerCpuData,
};
use alloc::boxed::Box;
use alloc::{string::String, vec::Vec};
use core::{
    arch::asm,
    mem::offset_of,
    sync::atomic::{AtomicU64, Ordering},
};
use hashbrown::HashMap;
use log::{error, info, trace, warn};
use spin::{Lazy, Mutex};
use x86_64::instructions::tlb::flush_all;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PhysFrame;
use x86_64::{structures::paging::PageTableFlags, PhysAddr, VirtAddr};
use xmas_elf::{header::Type as ElfType, program, program::Type as PhType, ElfFile};

// Choose a default userspace base for PIE/ET_DYN binaries
const DEFAULT_USER_BASE: u64 = 0x0000_0000_0040_0000; // 4 MiB, away from the null page(0x0)
const ELF_ADDR: u64 = 0x1000000 + KERNEL_BASE;

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    cr3: PhysAddr,
    end: u64,
    entry_point: u64,
    file_handles: HashMap<u32, FileHandle>,
    // Base address to load the binary at (0 for ET_EXEC, DEFAULT_USER_BASE for ET_DYN/PIE)
    load_bias: u64,
    loaded: bool,
}
impl Eq for Process {}
impl PartialEq for Process {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl Process {
    fn new(parent: &Process, name: String) -> Self {
        let cr3 = unsafe { create_cr3_from_current_page_tables() };

        let pid = NEXT_PID.load(Ordering::Acquire);
        let load_bias = 0;
        let mut file_handles = HashMap::new();
        file_handles.insert(
            0,
            FileHandle::new(0, Box::new(Stdin), FileOpenOptions::all()),
        );
        file_handles.insert(
            1,
            FileHandle::new(1, Box::new(Stdout), FileOpenOptions::all()),
        );
        file_handles.insert(
            2,
            FileHandle::new(2, Box::new(Stderr), FileOpenOptions::all()),
        );
        let p = Process {
            pid,
            parent_pid: parent.pid,
            state: ProcessState::new(),
            name,
            load_bias,
            end: 0,
            entry_point: 0,
            cr3,
            file_handles,
            loaded: false,
        };
        NEXT_PID.store(pid + 1, Ordering::Release);
        p
    }

    pub fn load(&mut self, bytes: &[u8]) {
        // CRITICAL: We need to modify the NEW process's memory.
        // Since ualloc_page and ptr::copy work on the ACTIVE CR3,
        // we must temporarily switch, do the work, and switch back.

        let (saved_cr3, flags) = Cr3::read();

        // 1. Switch to the new process context
        unsafe {
            Cr3::write(PhysFrame::containing_address(self.cr3), flags);
            flush_all();
        }

        // 2. Do the loading (allocating pages, copying ELF data)
        // This works because ELF_ADDR is in the Kernel (Upper) half,
        // which is shared/copied in create_cr3.
        self.internal_load(bytes);

        // 3. Switch BACK to the creator's context
        unsafe {
            Cr3::write(saved_cr3, flags);
        }
    }

    fn internal_load(&mut self, bytes: &[u8]) {
        let elf = ElfFile::new(bytes).expect("Failed to parse process");
        self.load_bias = compute_load_bias(&elf);
        let header = elf.header;
        info!("ELF header: {:?}", header);
        self.loaded = true;

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

                    info!(
                        "Copy: src={:#x}, dst={:#x}, len={:#x}",
                        src as u64, dst as u64, len
                    );

                    // Check if entry point will be in this segment
                    let entry = elf.header.pt2.entry_point();
                    if entry >= segment.virtual_addr()
                        && entry < segment.virtual_addr() + segment.file_size()
                    {
                        let offset_in_seg = (entry - segment.virtual_addr()) as usize;
                        let src_entry = unsafe { (src as *const u8).add(offset_in_seg) };
                        let src_entry_bytes = unsafe { core::slice::from_raw_parts(src_entry, 16) };
                        info!(
                            "Entry {:#x} is in this segment at offset {:#x}, source bytes: {:x?}",
                            entry, offset_in_seg, src_entry_bytes
                        );
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
        // Determine a safe user stack top: above the highest mapped segment + some gap
        let mut max_end = 0u64;
        for ph in elf.program_iter() {
            if let Ok(PhType::Load) = ph.get_type() {
                let end = ph.virtual_addr() + ph.mem_size();
                if end > max_end {
                    max_end = end;
                }
            }
        }
        self.end = max_end;
        self.entry_point = elf.header.pt2.entry_point();
    }

    pub fn prepare_run(&mut self) -> Option<(u64, u64)> {
        if !self.loaded {
            return None;
        }

        let new_cr3 = PhysFrame::containing_address(self.cr3);
        unsafe {
            Cr3::write(new_cr3, x86_64::registers::control::Cr3Flags::empty());
        }

        unsafe {
            CURRENT_PID.write(self.pid);
        }

        let max_end = self.end;

        let stack_gap = 0x20_000; // 128 KiB gap above image
        let stack_size = 0x4000; // 16 KiB user stack
        let user_stack_top =
            (self.load_bias + max_end + stack_gap + (PAGE_4K as u64 - 1)) & !((PAGE_4K as u64) - 1);

        info!(
            "Allocating user stack: max_end={:#x}, stack_top={:#x}, stack_size={:#x}",
            max_end, user_stack_top, stack_size
        );

        let stack_start = user_stack_top - stack_size;
        let mut addr = stack_start;
        while addr < user_stack_top {
            info!("Allocating stack page at {:#x}", addr);
            ualloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
            addr += PAGE_4K as u64;
        }

        self.state.rip = self.entry_point + self.load_bias;
        let user_stack = user_stack_top;
        let user_entry = self.state.rip;

        info!("Prepared process: {}", self.name);
        Some((user_entry, user_stack))
    }
    pub(crate) fn get_file_handle(&mut self, fd: u64) -> Option<&mut FileHandle> {
        self.file_handles.get_mut(&(fd as u32))
    }

    pub(crate) fn add_file_handle(
        &mut self,
        descriptor: Box<dyn File>,
        foo: FileOpenOptions,
    ) -> Result<u64, ()> {
        let fds = self.file_handles.keys();
        let max = fds.clone().max().cloned().unwrap_or(2);
        let new_fd = max + 1;
        let file_handle = FileHandle::new(new_fd, descriptor, foo);
        self.file_handles.insert(new_fd, file_handle);
        Ok(new_fd as u64)
    }

    pub(crate) fn close_file_handle(&mut self, fd: u64) -> Result<(), FileError> {
        self.file_handles
            .remove(&(fd as u32))
            .ok_or(FileError::InvalidFileDescriptor)?;
        Ok(())
    }

    pub fn trace(&self) {
        kprintln!("Process {} (pid {})", self.name, self.pid);
        kprintln!("  Parent PID: {}", self.parent_pid);
        kprintln!("  CR3: {:#x}", self.cr3.as_u64());
        kprintln!("  Load Bias: {:#x}", self.load_bias);
        kprintln!("  Entry Point: {:#x}", self.entry_point);
        kprintln!("  End Address: {:#x}", self.end);
        kprintln!("  State: {:?}", self.state);
        kprintln!("  File Handles:");
        for handle in self.file_handles.iter() {
            kprintln!("    FD {:#?}", handle);
        }
    }
}

unsafe impl Send for Process {}
unsafe impl Sync for Process {}

pub fn enter_user_mode(user_entry: u64, user_stack: u64) -> ! {
    let user_cs = (GDT.user_code_segment.0 | 3) as u64;
    let user_ss = (GDT.user_data_segment.0 | 3) as u64;
    let rflags = 0x202u64; // IF = 1

    info!("Switching to user mode");
    info!("Entry: {user_entry:x}");
    info!("Stack: {user_stack:x}");
    info!(
        "GDT user_code raw: {:#x}, user_data raw: {:#x}",
        GDT.user_code_segment.0, GDT.user_data_segment.0
    );
    info!("CS: {user_cs:x}");
    info!("SS: {user_ss:x}");
    info!("RFLAGS: {rflags:x}");

    assert_eq!(user_stack % 16, 0, "stack must be 16-byte aligned");

    // Build a user IRETQ frame and drop to ring 3
    unsafe {
        asm!(
        "cli",                    // be explicit; IF will be restored from RFLAGS
        "swapgs",
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
        );
    }
    unreachable!();
}

pub fn switch_to(process: &Process) {
    let mut p = PROCESSES.lock();
    let p_idx = p.iter().position(|p| p.pid == process.pid);
    if !p.contains(process) {
        warn!("Process {} not found in process list", process.pid);
    }

    if let Some(i) = p_idx {
        let proc = &mut p[i];
        let (entry, stack) = proc.prepare_run().unwrap();
        drop(p); // Release lock!
        enter_user_mode(entry, stack);
    }
}

#[repr(align(16))]
#[derive(Clone, Copy, Debug)]
struct FxSaveArea {
    fx_control_word: u16,
    fx_status_word: u16,
    fx_tag_word: u16,
    fx_opcode: u16,
    fx_eip: u32,
    fx_cs: u16,
    fx_reserved1: u16,
    fx_data_offset: u32,
    fx_ds: u16,
    fx_reserved2: u16,
    mxcsr: u32,
    mxcsr_mask: u32,
    st: [u8; 128],  // 8x 16-byte FPU/MMX registers
    xmm: [u8; 256], // 16x 16-byte XMM registers
    reserved: [u8; 96],
}

impl Default for FxSaveArea {
    fn default() -> Self {
        Self::new()
    }
}

impl FxSaveArea {
    fn new() -> Self {
        FxSaveArea {
            fx_control_word: 0,
            fx_status_word: 0,
            fx_tag_word: 0,
            fx_opcode: 0,
            fx_eip: 0,
            fx_cs: 0,
            fx_reserved1: 0,
            fx_data_offset: 0,
            fx_ds: 0,
            fx_reserved2: 0,
            mxcsr: 0,
            mxcsr_mask: 0,
            st: [0u8; 128],
            xmm: [0u8; 256],
            reserved: [0u8; 96],
        }
    }

    unsafe fn save(&mut self) {
        asm!(
        "fxsave [{}]",
        in(reg) self,
        options(nostack, preserves_flags)
        );
    }

    unsafe fn load(&self) {
        asm!(
        "fxrstor [{}]",
        in(reg) self,
        options(nostack, preserves_flags)
        );
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct ProcessState {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    rip: u64,
    rflags: u64,
    cs: u64,
    ds: u64,
    es: u64,
    fs: u64,
    gs: u64,
    ss: u64,
    fxsave: FxSaveArea,
}

impl ProcessState {
    fn new() -> Self {
        ProcessState {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
            rflags: 0,
            cs: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
            ss: 0,
            fxsave: Default::default(),
        }
    }

    unsafe fn cpy_regs(&mut self) {
        // general purpose registers
        asm!(
        "mov {}, rax",
        "mov {}, rbx",
        "mov {}, rcx",
        "mov {}, rdx",
        "mov {}, rsi",
        "mov {}, rdi",
        "mov {}, rbp",
        "mov {}, rsp",
        "pushfq",
        "pop {}",
        "mov {}, cs",
        "mov {}, ds",
        "mov {}, es",
        "mov {}, fs",
        "mov {}, gs",
        "mov {}, ss",
        out(reg) self.rax,
        out(reg) self.rbx,
        out(reg) self.rcx,
        out(reg) self.rdx,
        out(reg) self.rsi,
        out(reg) self.rdi,
        out(reg) self.rbp,
        out(reg) self.rsp,
        out(reg) self.rflags,
        out(reg) self.cs,
        out(reg) self.ds,
        out(reg) self.es,
        out(reg) self.fs,
        out(reg) self.gs,
        out(reg) self.ss,
        );

        // save FPU/SSE state
        self.fxsave.save();
    }

    unsafe fn load_regs(&self) {
        asm!(
        "mov rax, {}",
        "mov rbx, {}",
        "mov rcx, {}",
        "mov rdx, {}",
        "mov rsi, {}",
        "mov rdi, {}",
        "mov rbp, {}",
        "mov rsp, {}",
        in(reg) self.rax,
        in(reg) self.rbx,
        in(reg) self.rcx,
        in(reg) self.rdx,
        in(reg) self.rsi,
        in(reg) self.rdi,
        in(reg) self.rbp,
        in(reg) self.rsp,
        );

        // restore FPU/SSE state
        self.fxsave.load();
    }
}

pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init_process() -> &'static [u8] {
    let fs = FS.lock();
    let mut root = fs.root_dir().expect("Failed to get root dir");
    let mut bin_dir = root.open_dir("bin").expect("Failed to open bin dir");
    let mut file = match bin_dir.open_file("init.elf") {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open init: {:?}", e);
            error!("are you sure you are using a proper build?");
            panic!("Failed to open init");
        }
    };
    let len = get_len(file.as_mut()).unwrap_or(0);
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
    let elf = ElfFile::new(buf).expect("Failed to parse init");
    let cr3 = Cr3::read().0;
    let load_bias = compute_load_bias(&elf);

    let mut file_handles = HashMap::new();
    file_handles.insert(
        0,
        FileHandle::new(0, Box::new(Stdin), FileOpenOptions::all()),
    );
    file_handles.insert(
        1,
        FileHandle::new(1, Box::new(Stdout), FileOpenOptions::all()),
    );
    file_handles.insert(
        2,
        FileHandle::new(2, Box::new(Stderr), FileOpenOptions::all()),
    );
    let process = Process {
        pid: 0,
        parent_pid: u64::MAX,
        state: ProcessState::default(),
        name: String::from("init"),
        end: 0,
        entry_point: 0,
        load_bias,
        cr3: cr3.start_address(),
        file_handles,
        loaded: false,
    };
    PROCESSES.lock().push(process);
    buf
}

fn get_len(file: &mut dyn File) -> Result<u64, ()> {
    let current_pos = file.seek(SeekFrom::Current(0)).map_err(|e| {
        error!("Failed to get stream position: {:?}", e);
    })?;
    let end = file
        .seek(SeekFrom::End(0))
        .map_err(|e| error!("Seek failed: {:?}", e))?;
    file.seek(SeekFrom::Start(current_pos))
        .map_err(|e| error!("Seek restore failed: {:?}", e))?;
    Ok(end)
}
static CURRENT_PID: PerCpuVar<u64> = PerCpuVar::new(offset_of!(PerCpuData, curr_pid));
static SCHEDULER: Lazy<Mutex<Scheduler>> = Lazy::new(|| Mutex::new(Scheduler::new()));

// Helper to choose a per-process load bias for PIC/PIE binaries
fn compute_load_bias(elf: &ElfFile) -> u64 {
    match elf.header.pt2.type_().as_type() {
        ElfType::SharedObject => DEFAULT_USER_BASE,
        _ => 0,
    }
}

mod tests {
    use super::*;
    use crate::test_assert_eq as assert_eq;
    use crate::Test;

    #[zenos_macros::test]
    pub fn test_process_state_new() -> Option<()> {
        let state = ProcessState::new();
        assert_eq!(state.rax, 0);
        assert_eq!(state.rip, 0);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_process_state_default() -> Option<()> {
        let state = ProcessState::default();
        assert_eq!(state.rax, 0);
        assert_eq!(state.rip, 0);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_process_state_copy() -> Option<()> {
        let state1 = ProcessState {
            rax: 42,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            rip: 0x1000,
            rflags: 0,
            cs: 0,
            ds: 0,
            es: 0,
            fs: 0,
            gs: 0,
            ss: 0,
            fxsave: Default::default(),
        };
        let state2 = state1;
        assert_eq!(state1.rax, state2.rax);
        assert_eq!(state1.rip, state2.rip);
        assert_eq!(state1.rflags, state2.rflags);
        assert_eq!(state1.cs, state2.cs);
        assert_eq!(state1.ds, state2.ds);
        assert_eq!(state1.es, state2.es);
        assert_eq!(state1.fs, state2.fs);
        assert_eq!(state1.gs, state2.gs);
        assert_eq!(state1.ss, state2.ss);
        Some(())
    }
}
