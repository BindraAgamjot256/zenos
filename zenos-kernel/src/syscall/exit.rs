use crate::syscall::table::SyscallPtr;
use core::arch::asm;
use zenos_macros::syscall;

#[syscall(60)]
fn exit(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let exit_code = rdi as i32;
    log::info!("exit syscall called with code {}", exit_code);

    let proc = {
        let curr_pid = crate::process::current_pid();
        let mut procs = crate::process::PROCESSES.lock();
        let index = procs.iter().position(|p| p.pid == curr_pid);
        if index.is_none() {
            log::error!("exit: current process with pid {} not found", curr_pid);
            return 0;
        }
        let index = index.unwrap();
        let mut proc = procs.remove(index);
        proc.exit_code = Some(exit_code as u64);
        proc
    };
    {
        let mut procs = crate::process::PROCESSES.lock();
        for i in procs.iter_mut().filter(|p| p.parent_pid == proc.pid) {
            i.parent_pid = 1; // reparent to init process
        }
    };
    log::info!("process {} exited with code {}", exit_code, proc.pid);
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
