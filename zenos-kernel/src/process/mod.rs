use crate::fs::FS;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

struct Process {
    pid: u64,
    parent_pid: u64,
    name: String,
    state: ProcessState,
    code: Vec<u8>,
}

impl Process {
    fn new(parent: &Process, name: String, code: Vec<u8>) -> Self {
        Process {
            pid: parent.pid + 1,
            parent_pid: parent.pid,
            state: ProcessState::default(),
            name,
            code,
        }
    }
}

#[non_exhaustive]
#[derive(Default)]
struct ProcessState {
    rax: u64,
    rip: u64,
    cr3: u64, // TODO: replace with Cr3 struct
}

static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());

pub fn init_process() -> Process {
    let fs = FS.lock();
    let mut file = fs
        .root_dir()
        .open_dir("bin")
        .unwrap()
        .open_file("init")
        .unwrap();

    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    Process {
        pid: 0,
        parent_pid: 0,
        state: ProcessState::default(),
        name: String::from("init"),
        code: buf,
    }
}
