pub(crate) mod debug;
pub(crate) mod file_handles;
pub(crate) mod isolation;
pub(crate) mod scheduler;

pub use crate::process::scheduler::Scheduler;
use crate::{
    disk::FS,
    disk::FileError,
    disk::get_len,
    disk::vfs::File,
    framebuffer::FRAMEBUFFER,
    interrupts::gdt::GDT,
    kprintln,
    memory::ALLOCATOR,
    memory::{KERNEL_BASE, PAGE_4K, PageType, kalloc_page, ualloc_page, ualloc_page_flags},
    percpu::PerCpuData,
    percpu::PerCpuVar,
    process::debug::dump_pte,
    process::file_handles::{FileHandle, FileOpenOptions, Stderr, Stdin, Stdout},
    process::isolation::new_user_address_space,
};
use alloc::{boxed::Box, format, string::String, string::ToString, vec::Vec};
use core::{
    arch::asm,
    ffi::CStr,
    mem::offset_of,
    sync::atomic::AtomicBool,
    sync::atomic::{AtomicU64, Ordering},
};
use hashbrown::HashMap;
use heapless::Vec as HeaplessVec;
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

#[derive(Debug, Clone, Copy)]
pub enum ProcessStatus<'a> {
    /// Process is currently running on a CPU
    Running,
    /// Process is ready to run
    Ready,
    /// Process has not been prepared yet
    Created,
    /// Process has exited
    Exited,
    /// Process is blocked/waiting(e.g., on a lock. The AtomicBool indicates the lock(true = locked))
    Blocked(&'a AtomicBool),
    /// Process is waiting for another process to exit (stores target pid)
    WaitingFor(u64),
}

impl Default for ProcessStatus<'_> {
    fn default() -> Self {
        ProcessStatus::Created
    }
}

impl PartialEq<ProcessStatus<'_>> for ProcessStatus<'_> {
    fn eq(&self, other: &ProcessStatus<'_>) -> bool {
        match (self, other) {
            (ProcessStatus::Running, ProcessStatus::Running) => true,
            (ProcessStatus::Ready, ProcessStatus::Ready) => true,
            (ProcessStatus::Created, ProcessStatus::Created) => true,
            (ProcessStatus::Exited, ProcessStatus::Exited) => true,
            (ProcessStatus::Blocked(a), ProcessStatus::Blocked(b)) => {
                let a = a.load(Ordering::SeqCst);
                let b = b.load(Ordering::SeqCst);
                a == b
            }
            (ProcessStatus::WaitingFor(a), ProcessStatus::WaitingFor(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ProcessStatus<'_> {}

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    pub parent_pid: u64,
    pub name: String,
    pub state: ProcessState,
    pub status: ProcessStatus<'static>,
    pub cr3: PhysAddr,
    pub end: u64,
    pub entry_point: u64,
    pub file_handles: HashMap<u32, FileHandle>,
    /// Base address to load the binary at (0 for ET_EXEC, DEFAULT_USER_BASE for ET_DYN/PIE)
    pub load_bias: u64,
    pub loaded: bool,
    /// User stack top (saved for context switches)
    pub user_stack_top: u64,
    pub exit_code: Option<u64>,
    /// Environment variables for the process
    pub envp: Vec<Vec<u8>>,
    /// Argument vectors for the process
    pub argv: Vec<Vec<u8>>,
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
            FileHandle::new(0, Box::new(Stdin), FileOpenOptions::READ_WRITE),
        );
        file_handles.insert(
            1,
            FileHandle::new(1, Box::new(Stdout), FileOpenOptions::READ_WRITE),
        );
        file_handles.insert(
            2,
            FileHandle::new(2, Box::new(Stderr), FileOpenOptions::READ_WRITE),
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
            exit_code: None,
            envp: parent.envp.clone(),
            argv: Vec::new(),
        };
        NEXT_PID.store(pid + 1, Ordering::Release);
        p
    }

    pub fn exec_replace(
        &mut self,
        commandline: &str,
        argc: usize,
        argv: *const *const u8,
        envp: *const *const u8,
    ) {
        info!("Process {} execve: {}", self.pid, commandline);
        info!("  argc: {:#X}", argc);
        info!("  argv: {:#?}", argv);
        info!("  envp: {:#?}", envp);
        // Command line is already assembled by userspace.
        // We store it as-is for debugging / proc listings.
        let mut cmdline = commandline.to_string();

        // ---- ENVIRONMENT ----
        let mut new_envp: Vec<Vec<u8>> = Vec::new();

        if !envp.is_null() {
            unsafe {
                let mut i = 0;
                loop {
                    let ptr = *envp.add(i);
                    if ptr.is_null() {
                        break;
                    }

                    let cstr = CStr::from_ptr(ptr as *const i8);
                    new_envp.push(cstr.to_bytes().to_vec());

                    i += 1;
                }
            }
        }

        // ---- ARGUMENTS ----
        let mut new_argv: Vec<Vec<u8>> = Vec::new();

        if !argv.is_null() {
            unsafe {
                for i in 0..argc {
                    let ptr = *argv.add(i);
                    if ptr.is_null() {
                        continue;
                    }

                    let cstr = CStr::from_ptr(ptr as *const i8);
                    new_argv.push(cstr.to_bytes().to_vec());
                }
            }
        }

        // Optional polish: strip ".elf" from argv[0] in the *display name*
        if let Some(first) = new_argv.first() {
            if let Ok(s) = core::str::from_utf8(first) {
                if let Some(stripped) = s.strip_suffix(".elf") {
                    // Replace only the leading token in the command line
                    if let Some(rest) = cmdline.strip_prefix(s) {
                        cmdline = format!("{}{}", stripped, rest);
                    }
                }
            }
        }

        // ---- COMMIT PROCESS STATE ----
        self.argv = new_argv;
        self.envp = new_envp;
        self.name = cmdline;

        self.state = ProcessState::default();
        self.status = ProcessStatus::Created;

        self.load_bias = 0;
        self.end = 0;
        self.entry_point = 0;
        self.loaded = false;
        self.user_stack_top = 0;

        self.file_handles
            .retain(|_, fh| !fh.foo.contains(FileOpenOptions::CLOSE_ON_EXEC));

        unsafe {
            self.cr3 = new_user_address_space().unwrap();
        }
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
                    let ptf = PageTableFlags::PRESENT
                        | PageTableFlags::USER_ACCESSIBLE
                        | PageTableFlags::WRITABLE;
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
                    dump_pte(VirtAddr::new(dst as u64));
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
                    } else {
                        ptf &= !(PageTableFlags::NO_EXECUTE | PageTableFlags::WRITABLE);
                    }
                    let mut addr = page_start;
                    while addr < page_end {
                        // change_flags(addr, ptf).unwrap();
                        // fuck w^x again, because it doesn't work in tandem with CoW.
                        addr += PAGE_4K as u64;
                    }
                    #[cfg(debug_assertions)]
                    dump_pte(VirtAddr::new(dst as u64));

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

        // 1. Switch Address Space
        let new_cr3 = PhysFrame::containing_address(self.cr3);
        unsafe {
            Cr3::write(new_cr3, x86_64::registers::control::Cr3Flags::empty());
            CURRENT_PID.write(self.pid);
        }

        // 2. Setup Stack (only once)
        if self.user_stack_top == 0 {
            info!("Preparing user stack for process {}", self.pid);
            let stack_gap = 0x20_000;
            let stack_size = 0x4000; // 16 KiB
            let user_stack_top = (self.load_bias + self.end + stack_gap + 0xFFF) & !0xFFF;
            let stack_start = user_stack_top - stack_size;

            // Map the stack
            let mut addr = stack_start;
            while addr < user_stack_top {
                ualloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
                addr += 4096;
            }

            let mut sp = user_stack_top;
            assert!(sp > stack_start, "stack grows downwards");
            info!(
                "User stack allocated at {:#x}-{:#x}",
                stack_start, user_stack_top
            );
            info!(
                "Preparing user stack for process {}, sp:{:#x}",
                self.pid, sp
            );

            // Phase 1: Copy Strings
            let mut envp_ptrs = Vec::new();
            for env in self.envp.iter().rev() {
                sp -= (env.len() + 1) as u64; // +1 for null terminator
                unsafe {
                    core::ptr::copy_nonoverlapping(env.as_ptr(), sp as *mut u8, env.len());
                    *(sp as *mut u8).add(env.len()) = 0;
                }
                envp_ptrs.push(sp);
            }

            let mut argv_ptrs = Vec::new();
            for arg in self.argv.iter().rev() {
                sp -= (arg.len() + 1) as u64; // +1 for null terminator
                info!(
                    "Pushing arg '{}' at {:#x}",
                    String::from_utf8_lossy(arg),
                    sp
                );
                unsafe {
                    core::ptr::copy_nonoverlapping(arg.as_ptr(), sp as *mut u8, arg.len());
                    *(sp as *mut u8).add(arg.len()) = 0;
                }
                argv_ptrs.push(sp);
            }

            while sp % 16 != 0 {
                sp -= 1; // Align to 16 bytes
            }
            // --- PHASE 2: Alignment and Pointers ---
            // Calculate how many 8-byte entries we will push
            // argc (1) + argv ptrs (N) + envp ptrs (M) + null (1) + auxv null (2)
            let pointer_entries = 0 +
                    1 +                    // argc
                    self.argv.len() +      // argv pointers
                    self.envp.len() +      // envp pointers
                    2; // AT_NULL (type, value)
            let total_pointer_size = (pointer_entries * 8) as u64;

            // Align sp such that after pushing all pointers, sp is 16-byte aligned
            let temp_sp = sp - total_pointer_size;
            if temp_sp % 16 != 0 {
                sp -= 8; // Adjust for 16-byte alignment
            }

            unsafe {
                let push = |val: u64, stack_ptr: &mut u64| {
                    *stack_ptr -= 8;
                    *(*stack_ptr as *mut u64) = val;
                };

                // 1. Auxiliary Vector (Minimal: AT_NULL)
                push(0, &mut sp); // AT_NULL value
                push(0, &mut sp); // AT_NULL type

                // 2. Envp
                push(0, &mut sp); // Env terminator
                for ptr in envp_ptrs {
                    push(ptr, &mut sp);
                }

                // 3. Argv
                push(0, &mut sp); // Arg terminator
                for ptr in argv_ptrs {
                    push(ptr, &mut sp);
                }

                // 4. Argc
                push(self.argv.len() as u64, &mut sp);
            }

            // Ensure we didn't overflow our allocated stack
            assert!(sp >= stack_start, "User stack overflow during preparation!");

            self.state.rsp = sp;
            self.state.rip = self.entry_point + self.load_bias;
            self.user_stack_top = user_stack_top;
            assert_eq!(self.state.rsp % 16, 0, "User stack not 16-byte aligned!");
        }

        self.status = ProcessStatus::Running;
        Some((self.state.rip, self.state.rsp))
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
        "xor rax, rax",        // clear RAX
        "xor rdi, rdi",        // clear RDI
        "xor rsi, rsi",        // clear RSI
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

/// Switch to the next ready process immediately.
/// Used by exit syscall to avoid busy-waiting.
/// Never returns if a process is found to switch to.
pub fn schedule_next() -> ! {
    // Force the scheduler to pick a new process on the next timer tick
    {
        let mut sched = SCHEDULER.lock();
        sched.force_reschedule();
        let proc = PROCESSES.lock().len();
        info!("schedule_next: {} processes in the system", proc);
        drop(sched);
    }

    unsafe {
        PROCESSES.force_unlock();
        FS.force_unlock();
        FRAMEBUFFER.force_unlock();
        ALLOCATOR.force_unlock();
    }

    // Enable interrupts and halt - the next timer tick will context switch
    unsafe {
        asm!("sti", "hlt", options(nomem, nostack),);
    }

    // Loop halting until timer reschedules us away
    loop {
        unsafe { asm!("hlt") };
    }
}

pub fn block_current_process(lock: &'static AtomicBool) {
    let curr_pid = current_pid();
    let mut procs = PROCESSES.lock();
    if let Some(proc) = procs.iter_mut().find(|p| p.pid == curr_pid) {
        proc.status = ProcessStatus::Blocked(lock);
        trace!("Process {} blocked", curr_pid);
    } else {
        warn!(
            "block_current_process: no process found with pid {}",
            curr_pid
        );
    }
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
    pub fn new() -> Self {
        let mut area = FxSaveArea { _data: [0; 512] };
        area.save();
        area
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

pub static PROCESSES: Mutex<HeaplessVec<Process, 256>> = Mutex::new(HeaplessVec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(2);

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
        pid: 1,
        parent_pid: 0,
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
        exit_code: None,
        envp: Vec::new(),
        argv: Vec::new(),
    };
    PROCESSES.lock().push(process).unwrap();
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
    pub fn test_fxsave_area_alignment() -> Option<()> {
        // FxSave requires 16-byte alignment
        crate::test_assert!(align_of::<FxSaveArea>() >= 16);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_fxsave_area_size() -> Option<()> {
        // FxSave area should be 512 bytes
        assert_eq!(size_of::<FxSaveArea>(), 512);
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
