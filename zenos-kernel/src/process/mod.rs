use crate::fs::FS;
use crate::interrupts::gdt::GDT;
use crate::kprintln;
use crate::memory::{PAGE_4K, PageType, kalloc_page, ualloc_page, ualloc_page_flags};
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};
use fatfs::{Read, Seek, SeekFrom};
use log::{error, info, trace};
use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::registers::rflags::RFlags;
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::idt::InterruptStackFrame;
use x86_64::structures::paging::PageTableFlags;
use xmas_elf::program;
use xmas_elf::program::Type;

const PROCESS_ADDR: u64 = 0x2000000;
const ELF_ADDR: u64 = 0x1000000;

#[derive(Debug)]
pub struct Process<'a> {
    pub pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    code: xmas_elf::ElfFile<'a>,
}

impl<'a> Process<'a> {
    fn new(parent: &Process, name: String, code: &'a [u8]) -> Self {
        let pid = NEXT_PID.load(Ordering::Acquire);
        let p = Process {
            pid,
            parent_pid: parent.pid,
            state: ProcessState::new(),
            name,
            code: xmas_elf::ElfFile::new(code).expect("Failed to parse process"),
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
                Ok(Type::Load) => {
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
                    ptf |= PageTableFlags::WRITABLE; // temporarily fuck W^X... don't tell anyone 🤫
                    let mem_size = segment.mem_size();
                    let seg_start = VirtAddr::new(segment.virtual_addr() + PROCESS_ADDR);
                    let seg_end = VirtAddr::new(segment.virtual_addr() + mem_size + PROCESS_ADDR);
                    let page_start = seg_start.align_down(PAGE_4K as u64);
                    let page_end = seg_end.align_up(PAGE_4K as u64);

                    let mut addr = page_start;
                    while addr < page_end {
                        ualloc_page_flags(addr, PageType::Arbitrary, ptf).unwrap();
                        addr += PAGE_4K as u64;
                    }
                    let src = (ELF_ADDR + segment.offset()) as *mut u8; // don't you just love pointer gymnastics?
                    let dst =
                        ((segment.virtual_addr() as *mut u8) as u64 + PROCESS_ADDR) as *mut u8;
                    let len = segment.file_size() as usize;

                    unsafe {
                        core::ptr::copy(src, dst, len); // ptr overlaps with the dst, so it will cause a err in copy_nonoverlapping.
                    }
                    info!(
                        "Segment {} copied to {seg_start:x}-{seg_end:x}",
                        segment.virtual_addr()
                    );
                    let bss_len = segment.mem_size() - segment.file_size();
                    if bss_len > 0 {
                        unsafe {
                            core::ptr::write_bytes(
                                (segment.virtual_addr() + segment.file_size() + PROCESS_ADDR)
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

    pub fn run(&mut self) -> Result<(), ()> {
        if !self.state.loaded {
            return Err(());
        }
        let user_stack_top = PROCESS_ADDR + 0x0100_0000;
        for addr in (user_stack_top - 0x4000..user_stack_top).step_by(PAGE_4K) {
            ualloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
        }
        self.state.rip = self.code.header.pt2.entry_point() + PROCESS_ADDR;
        let user_stack = user_stack_top;
        let user_entry = self.state.rip;
        let user_cs = (GDT.user_code_segment.0 | 3) as u64;
        let user_ss = (GDT.user_data_segment.0 | 3) as u64;
        let rflags = 0x202u64; // IF = 1

        info!("Switching to process: {}", self.name);
        info!("Entry: {user_entry:x}");
        info!("CS: {user_cs:x}");
        info!("SS: {user_ss:x}");
        info!("RFLAGS: {rflags:x}");
        assert_eq!(user_stack % 16, 0, "stack must be 16-byte aligned");
        unsafe {
            core::arch::asm!(
            "jmp {0}",
            in(reg) jmp_userland,
            in("rax") user_ss,
            in("r11") user_stack,
            in("rcx") rflags,
            in("r8") user_cs,
            in("r9") user_entry,
            )
        }
        Ok(())
    }

    pub fn trace(&self) {
        kprintln!("{:#?}", self.state);
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
    if let Some(i) = p_idx {
        let proc = &mut p[i];
        proc.run().unwrap();
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

unsafe extern "C" fn jmp_userland() {}

global_asm!(
    "
.global jmp_userland
jmp_userland:
    mov rbp, 0
    mov rsp, r11
    push rax
    push r11
    push rcx
    push r8
    push r9
    iretq
"
);

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
    if let Err(e) = file.read(buf) {
        error!("Failed to read init: {:?}", e);
    }

    trace!("init code copied to 0x{:x}", ELF_ADDR);
    let process = Process {
        pid: 0,
        parent_pid: u64::MAX,
        state: ProcessState::default(),
        name: String::from("init"),
        code: xmas_elf::ElfFile::new(buf).expect("Failed to parse init"),
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
