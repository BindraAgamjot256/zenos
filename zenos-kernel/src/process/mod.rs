use crate::fs::FS;
use crate::kprintln;
use crate::memory::{PAGE_4K, PageType, kalloc_page};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use fatfs::{Read, Seek, SeekFrom};
use log::{error, trace};
use spin::Mutex;
use x86_64::VirtAddr;

const PROCESS_ADDR: u64 = 0x2000000;

#[derive(Clone)]
pub struct Process<'a> {
    pub pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    code: &'a [u8],
}

impl<'a> Process<'a> {
    fn new(parent: &Process, name: String, code: &'a [u8]) -> Self {
        let pid = NEXT_PID.load(Ordering::Acquire);
        let p = Process {
            pid,
            parent_pid: parent.pid,
            state: ProcessState::default(),
            name,
            code,
        };
        NEXT_PID.store(pid + 1, Ordering::Release);
        p
    }

    pub fn load(&self) {
        let elf = xmas_elf::ElfFile::new(&self.code).unwrap();
        let header = elf.header;
        log::info!("ELF header: {:?}", header);
    }
    pub fn trace(&self) {
        kprintln!("{:#?}", self.state);
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct ProcessState {
    rax: u64,
    rip: u64,
    cr3: u64,
}

static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init_process() -> Process<'static> {
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
            VirtAddr::new(PROCESS_ADDR + (page * PAGE_4K as u64)),
            PageType::Arbitrary,
        )
        .expect("Map failure");
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(PROCESS_ADDR as *mut u8, len as usize) };
    trace!("len: {} bytes", len);
    if let Err(e) = file.read(buf) {
        error!("Failed to read init: {:?}", e);
    }

    trace!("init code copied to 0x{:x}", PROCESS_ADDR);
    let process = Process {
        pid: 0,
        parent_pid: u64::MAX,
        state: ProcessState::default(),
        name: String::from("init"),
        code: buf,
    };
    PROCESSES.lock().push(process.clone());
    process
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
