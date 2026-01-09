use log::info;
use spin::Lazy;

// We define the signature for the syscall handler
pub type SyscallFn = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

// #[repr(C)] guarantees the order: 'id' is always first, 'handler' is always second.
#[repr(C)]
pub struct SyscallPtr {
    pub id: usize,
    pub handler: SyscallFn,
}

unsafe extern "C" {
    static __start_syscall_table: SyscallPtr;
    static __stop_syscall_table: SyscallPtr;
}

unsafe fn build_syscall_table() -> [Option<SyscallFn>; 256] {
    let mut table: [Option<SyscallFn>; 256] = [None; 256];

    let mut current = &__start_syscall_table as *const SyscallPtr;
    let stop = &__stop_syscall_table as *const SyscallPtr;

    while current < stop {
        // Dereference the pointer to get the struct fields
        // Since it's #[repr(C)], this read is safe and aligned
        let entry = &*current;

        if entry.id < 256 {
            table[entry.id] = Some(entry.handler);
        } else {
            panic!("invalid syscall table entry");
        }

        // Move to next entry
        current = current.add(1);
    }

    table
}

pub(crate) static SYSCALL_TABLE: Lazy<[Option<SyscallFn>; 256]> = Lazy::new(|| {
    let tab = unsafe { build_syscall_table() };
    info!("Syscall table initialized, {:?}", tab);
    tab
});
