mod debug;
pub(crate) mod file_handles;
pub(crate) mod isolation;
pub(crate) mod scheduler;

pub use crate::process::scheduler::Scheduler;
use crate::{
    disk::FS,
    disk::FileError,
    disk::get_len,
    disk::vfs::File,
    interrupts::gdt::GDT,
    kprintln,
    memory::change_flags,
    memory::{KERNEL_BASE, PAGE_4K, PageType, kalloc_page, ualloc_page, ualloc_page_flags},
    percpu::PerCpuData,
    percpu::PerCpuVar,
    process::debug::dump_pte,
    process::file_handles::{FileHandle, FileOpenOptions, Stderr, Stdin, Stdout},
    process::isolation::new_user_address_space,
};
use alloc::string::ToString;
use alloc::{boxed::Box, string::String, vec::Vec};
use core::{
    arch::asm,
    mem::offset_of,
    sync::atomic::{AtomicU64, Ordering},
};
use hashbrown::HashMap;
use log::{LevelFilter, error, info, trace, warn};
use spin::{Lazy, Mutex};
use x86_64::{
    PhysAddr, VirtAddr, instructions::tlb::flush_all, registers::control::Cr3,
    structures::paging::PageTableFlags, structures::paging::PhysFrame,
};
use xmas_elf::{ElfFile, header::Type as ElfType, program, program::Type as PhType};

// Choose a default userspace base for PIE/ET_DYN binaries
const DEFAULT_USER_BASE: u64 = 0x0000_0000_0040_0000; // 4 MiB, away from the null page(0x0)
const ELF_ADDR: u64 = 0x1000000 + KERNEL_BASE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is currently running on a CPU
    Running,
    /// Process is ready to run
    Ready,
    /// Process has not been prepared yet
    Created,
    /// Process has exited
    Exited,
}

impl Default for ProcessStatus {
    fn default() -> Self {
        ProcessStatus::Created
    }
}

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    pub parent_pid: u64,
    pub name: String,
    pub state: ProcessState,
    pub status: ProcessStatus,
    pub cr3: PhysAddr,
    pub end: u64,
    pub entry_point: u64,
    pub file_handles: HashMap<u32, FileHandle>,
    /// Base address to load the binary at (0 for ET_EXEC, DEFAULT_USER_BASE for ET_DYN/PIE)
    pub load_bias: u64,
    pub loaded: bool,
    /// User stack top (saved for context switches)
    pub user_stack_top: u64,
}
impl Eq for Process {}
impl PartialEq for Process {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl Process {
    fn new(parent: &Process, name: String) -> Self {
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
            state: ProcessState::default(),
            status: ProcessStatus::Created,
            name,
            load_bias,
            end: 0,
            entry_point: 0,
            cr3: Cr3::read().0.start_address(),
            file_handles,
            loaded: false,
            user_stack_top: 0,
        };
        NEXT_PID.store(pid + 1, Ordering::Release);
        p
    }

    pub fn exec_replace(&mut self, name: &str) {
        let s = name.to_string();

        let trimmed = s.strip_suffix(".elf").map(|x| x.to_string()).unwrap_or(s);

        self.name = trimmed;
        self.state = ProcessState::default();
        self.status = ProcessStatus::Created;
        self.load_bias = 0;
        self.end = 0;
        self.entry_point = 0;
        self.loaded = false;
        self.user_stack_top = 0;
        self.file_handles = self
            .file_handles
            .drain()
            .filter(|(_, fh)| !fh.foo.contains(FileOpenOptions::CLOSE_ON_EXEC))
            .collect();
        unsafe { self.cr3 = new_user_address_space().unwrap() }
    }

    /// Create a child process by forking from a parent
    /// Returns the child process with cloned address space and state
    pub fn fork_from(parent: &Process, child_state: ProcessState) -> Result<Self, ()> {
        use crate::process::isolation::clone_address_space;

        // Deep copy the parent's address space (all user pages are copied)
        let child_cr3 =
            unsafe { clone_address_space() }.map_err(|_| error!("Failed to clone child cr3"))?;

        // Create child process and copy parent's metadata
        let mut p = Process::new(parent, parent.name.clone());
        p.cr3 = child_cr3;
        p.state = child_state;
        p.load_bias = parent.load_bias;
        p.end = parent.end;
        p.entry_point = parent.entry_point;
        p.user_stack_top = parent.user_stack_top;
        p.loaded = true; // Already loaded via cloned address space
        p.status = ProcessStatus::Ready;
        Ok(p)
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
                        log::set_max_level(LevelFilter::Trace);
                        ualloc_page_flags(addr, PageType::Arbitrary, ptf).unwrap();
                        log::set_max_level(LevelFilter::Debug);
                        addr += PAGE_4K as u64;
                    }
                    let src = unsafe { bytes.as_ptr().add(segment.offset() as usize) };
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
                    // dump page tables for debugging
                    #[cfg(debug_assertions)]
                    dump_pte(VirtAddr::new(0x200000));
                    unsafe {
                        core::ptr::copy(src, dst, len);
                    }
                    info!(
                        "Segment {} copied to {:#x}-{:#x}",
                        segment.virtual_addr(),
                        seg_start.as_u64(),
                        seg_end.as_u64()
                    );

                    let mut ptf = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
                    if flags.is_write() {
                        ptf |= PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE
                    }
                    let mut addr = page_start;
                    while addr < page_end {
                        change_flags(addr, ptf).unwrap();
                        addr += PAGE_4K as u64;
                    }

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

        // Only allocate stack if not already allocated
        if self.user_stack_top == 0 {
            let max_end = self.end;

            let stack_gap = 0x20_000; // 128 KiB gap above image
            let stack_size = 0x4000; // 16 KiB user stack
            let user_stack_top = (self.load_bias + max_end + stack_gap + (PAGE_4K as u64 - 1))
                & !((PAGE_4K as u64) - 1);

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

            self.user_stack_top = user_stack_top;
            self.state.rip = self.entry_point + self.load_bias;
            self.state.rsp = user_stack_top;
        }

        let user_stack = self.state.rsp;
        let user_entry = self.state.rip;

        self.status = ProcessStatus::Running;

        info!("Prepared process: {}", self.name);
        Some((user_entry, user_stack))
    }

    /// Save CPU context into this process (called from timer interrupt)
    pub fn save_context(&mut self, ctx: &ProcessState) {
        self.state = *ctx;
        self.status = ProcessStatus::Ready;
    }

    /// Get the saved context for resuming
    pub fn get_context(&self) -> &ProcessState {
        &self.state
    }

    /// Get CR3 for this process
    pub fn get_cr3(&self) -> PhysAddr {
        self.cr3
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
/// CPU context saved during interrupt/context switch
/// Must match the layout expected by the assembly timer handler
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ProcessState {
    // General purpose registers (saved by assembly handler)
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    // Instruction pointer and flags (from interrupt frame)
    pub rip: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
    // FPU/SSE state
    pub fxsave: FxSaveArea,
}

#[repr(align(16))]
#[derive(Clone, Copy, Debug)]
pub struct FxSaveArea {
    _data: [u8; 512],
}
impl Default for FxSaveArea {
    fn default() -> Self {
        FxSaveArea::new()
    }
}

impl FxSaveArea {
    pub const fn new() -> Self {
        FxSaveArea { _data: [0; 512] }
    }
    pub fn save(&mut self) {
        unsafe {
            asm!(
                "fxsave [{}]",
                in(reg) &mut self._data,
                options(nostack, preserves_flags),
            );
        }
    }
}

pub static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init_process() -> &'static [u8] {
    let fs = FS.lock();
    let mut file = match fs.open_file("/bin/init.elf") {
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
        status: ProcessStatus::Created,
        name: String::from("/bin/init"),
        end: 0,
        entry_point: 0,
        load_bias,
        cr3: cr3.start_address(),
        file_handles,
        loaded: false,
        user_stack_top: 0,
    };
    PROCESSES.lock().push(process);
    buf
}

static CURRENT_PID: PerCpuVar<u64> = PerCpuVar::new(offset_of!(PerCpuData, curr_pid));
pub static SCHEDULER: Lazy<Mutex<Scheduler>> = Lazy::new(|| Mutex::new(Scheduler::new()));

/// Get the current process PID
pub fn current_pid() -> u64 {
    unsafe { CURRENT_PID.read() }
}

/// Set the current process PID
pub fn set_current_pid(pid: u64) {
    unsafe { CURRENT_PID.write(pid) }
}

// Helper to choose a per-process load bias for PIC/PIE binaries
fn compute_load_bias(elf: &ElfFile) -> u64 {
    match elf.header.pt2.type_().as_type() {
        ElfType::SharedObject => DEFAULT_USER_BASE,
        _ => 0,
    }
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;
    use crate::test_assert_eq as assert_eq;

    #[zenos_macros::test]
    pub fn test_process_state_new() -> Option<()> {
        let state = ProcessState::default();
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
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rbp: 0,
            rsp: 0,
            rip: 0x1000,
            rflags: 0,
            cs: 0,
            ss: 0,
            fxsave: Default::default(),
        };
        let state2 = state1;
        assert_eq!(state1.rax, state2.rax);
        assert_eq!(state1.rip, state2.rip);
        assert_eq!(state1.rflags, state2.rflags);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_process_state_all_registers_zero() -> Option<()> {
        let state = ProcessState::default();
        assert_eq!(state.rax, 0);
        assert_eq!(state.rbx, 0);
        assert_eq!(state.rcx, 0);
        assert_eq!(state.rdx, 0);
        assert_eq!(state.rsi, 0);
        assert_eq!(state.rdi, 0);
        assert_eq!(state.rbp, 0);
        assert_eq!(state.rsp, 0);
        assert_eq!(state.rip, 0);
        assert_eq!(state.rflags, 0);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_fxsave_area_default() -> Option<()> {
        let fx = FxSaveArea::default();
        // FxSave area should be zero-initialized
        for byte in fx._data.iter() {
            assert_eq!(*byte, 0);
        }
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_fxsave_area_alignment() -> Option<()> {
        // FxSave requires 16-byte alignment
        crate::test_assert!(core::mem::align_of::<FxSaveArea>() >= 16);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_fxsave_area_size() -> Option<()> {
        // FxSave area should be 512 bytes
        assert_eq!(core::mem::size_of::<FxSaveArea>(), 512);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_default_user_base_not_zero() -> Option<()> {
        // User base should not be at NULL to catch null pointer dereferences
        crate::test_assert!(DEFAULT_USER_BASE > 0);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_default_user_base_page_aligned() -> Option<()> {
        // User base should be page-aligned
        assert_eq!(DEFAULT_USER_BASE % 4096, 0);
        Some(())
    }
}
