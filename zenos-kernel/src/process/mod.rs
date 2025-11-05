use crate::fs::FS;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use fatfs::{Read, Seek, SeekFrom};
use log::{error, trace};
use spin::Mutex;


const PROCESS_ADDR: u64 = 0x2000000;

#[derive(Clone)]
pub struct Process<'a> {
    pub pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    code: & 'a [u8],
}

impl<'a> Process<'a> {
    fn new(parent: &Process, name: String, code: &'a[u8]) -> Self {
        let pid = NEXT_PID.lock();
        Process {
            pid: *pid,
            parent_pid: parent.pid,
            state: ProcessState::default(),
            name,
            code,
        }
    }
    
    pub fn load(&self) {
        let elf = xmas_elf::ElfFile::new(&self.code).unwrap();
        let header = elf.header;
        log::info!("ELF header: {:?}", header);
        
    }
}

#[derive(Default, Clone, Copy)]
struct ProcessState {
    rax: u64,
    rip: u64,
    cr3: u64,
}

static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: Mutex<u64> = Mutex::new(1);

pub fn init_process() -> Process<'static> {
    let fs = FS.lock();
    let mut file = match fs.root_dir().open_dir("bin").and_then(|b| b.open_file("init.elf")) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open init: {:?}", e);
            error!("are you sure you are using a proper build?");
            panic!("Failed to open init");
        }
    };
    let len = get_len(&mut file).unwrap_or(0);
    let mut buf = unsafe { core::slice::from_raw_parts_mut(PROCESS_ADDR as *mut u8, len as usize) };
    trace!("len: {} bytes", len);
    if let Err(e) = file.read(buf) {
        error!("Failed to read init: {:?}", e);
    }
    
    trace!("init code copied to 0x{:x}", PROCESS_ADDR);
    let process = Process {
        pid: 0,
        parent_pid: 0,
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
    let end = obj.seek(SeekFrom::End(0)).map_err(|e| error!("Seek failed: {:?}", e))?;
    obj.seek(SeekFrom::Start(current_pos)).map_err(|e| error!("Seek restore failed: {:?}", e))?;
    Ok(end)
}
