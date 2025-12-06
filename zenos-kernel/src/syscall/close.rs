use crate::syscall::table::SyscallPtr;
use syscall_macro::syscall;

#[syscall(3)]
fn close(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let ret = 0;
    let fd = rdi;
    let mut processes = crate::process::PROCESSES.lock();
    let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let process = processes.iter_mut().find(|process| process.pid == curr_pid);
    if process.is_none() {
        return u64::MAX;
    }
    let process = process.unwrap();
    let res = process.close_file_handle(fd);
    if res.is_err() {
        return u64::MAX;
    }
    ret
}
