use crate::process::ProcessStatus;
use crate::syscall::table::SyscallPtr;
use zenos_macros::syscall;

//noinspection RsDropRef
#[syscall(60)]
fn exit(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let exit_code = rdi as i32;
    log::info!("exit syscall called with code {}", exit_code);

    {
        let curr_pid = crate::process::current_pid();
        let mut procs = crate::process::PROCESSES.lock();

        if let Some(proc) = procs.iter_mut().find(|p| p.pid == curr_pid) {
            // Mark as exited (zombie) instead of removing - parent needs to wait() to reap
            proc.exit_code = Some(exit_code as u64);
            proc.status = ProcessStatus::Exited;
            log::info!("process {} exited with code {}", curr_pid, exit_code);

            // Reparent children to init
            let pid = proc.pid;
            // I know that technically this should have no effect, but it's done so that I can
            // re-borrow procs mutably again, which is needed to reparent children.
            #[allow(dropping_references)]
            drop(proc); // release the mutable borrow
            for child in procs.iter_mut().filter(|p| p.parent_pid == pid) {
                child.parent_pid = 1;
            }
        } else {
            log::error!("exit: current process with pid {} not found", curr_pid);
            return 0;
        }
    }

    // Immediately switch to the next ready process
    crate::process::schedule_next();
}
