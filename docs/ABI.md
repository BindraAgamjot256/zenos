# Zenos Kernel ABI Documentation

This document describes the kernel Application Binary Interface (ABI) for Zenos, including how the kernel initializes, memory layout, system call conventions, and process execution model.

## Table of Contents

1. [Boot and Initialization](#boot-and-initialization)
2. [Memory Layout](#memory-layout)
3. [System Call ABI](#system-call-abi)
4. [Process Execution Model](#process-execution-model)
5. [User Stack Layout](#user-stack-layout)
6. [C Runtime (crt0)](#c-runtime-crt0)
7. [Context Switching](#context-switching)

---

## Boot and Initialization

Zenos boots via a custom UEFI bootloader that hands control to the kernel's entry point (`kmain`). The initialization sequence is:

### 1. Memory Initialization
- Serial logger is configured for debug output
- Memory regions are discovered from the bootloader's `BootInfo` structure
- The page allocator is initialized with a bitmap-based free list
- The slab allocator is initialized for small kernel allocations

### 2. Framebuffer & TTY
- Framebuffer is initialized from bootloader-provided info
- TTY subsystem is set up with ANSI escape sequence support

### 3. CPU Structures
- **IDT (Interrupt Descriptor Table)**: Interrupt handlers are registered
- **GDT (Global Descriptor Table)**: Segments are configured:
  - Kernel Code Segment (Ring 0)
  - Kernel Data Segment (Ring 0)
  - User Code Segment (Ring 3)
  - User Data Segment (Ring 3)
  - TSS (Task State Segment)
- **TSS**: Privilege stacks (RSP0) and interrupt stacks (IST) are configured

### 4. Hardware Initialization
- ACPI tables are parsed for hardware discovery
- APIC (Local + I/O) is initialized for interrupt routing
- Per-CPU data structures are initialized via `SWAPGS` mechanism
- Syscall MSRs are configured (STAR, LSTAR, EFER)
- FPU/SSE is enabled (CR0.EM=0, CR0.MP=1, CR4.OSFXSR=1)

### 5. Filesystem & Processes
- Disk subsystem and VFS are initialized
- Kernel idle task (PID 0) is created
- Init process (PID 1) is loaded from `/bin/init`
- Control transfers to user mode

---

## Memory Layout

Zenos uses a higher-half kernel with the following virtual address layout:

| Address Range                                     | Purpose                                                     |
|---------------------------------------------------|-------------------------------------------------------------|
| `0x0000_0000_0040_0000`                           | Default user space base (for PIE binaries)                  |
| `0x0000_0000_0000_0000` - `0x0000_7FFF_FFFF_FFFF` | User space                                                  |
| `0xFFFF_8000_0000_0000`                           | Higher-half base (PML4 entry 256) - Physical memory mapping |
| `0xFFFF_FF00_0000_0000`                           | Kernel mapping base (PML4 entry 510)                        |
| `0xFFFF_FF00_0000_0000`                           | Kernel code/data base                                       |
| `0xFFFF_FF01_0000_0000`                           | Kernel stack base                                           |
| `0xFFFF_FF02_0000_0000`                           | Framebuffer mappings                                        |
| `0xFFFF_FF10_0000_0000`                           | Slab allocator base                                         |
| `0xFFFF_FF40_0000_0000`                           | Large allocations base                                      |
| `0xFFFF_FF70_0000_0000`                           | CR3 scratch space (for page table manipulation)             |

### Page Sizes
- **4 KiB**: Standard pages for user and kernel allocations
- **2 MiB**: Huge pages for large mappings

---

## System Call ABI

Zenos uses the x86_64 `syscall` instruction for user-to-kernel transitions, following the Linux/System V AMD64 syscall convention.

### Register Convention

| Register | Purpose                                        |
|----------|------------------------------------------------|
| `RAX`    | Syscall number (input) / Return value (output) |
| `RDI`    | Argument 1                                     |
| `RSI`    | Argument 2                                     |
| `RDX`    | Argument 3                                     |
| `R10`    | Argument 4                                     |
| `R8`     | Argument 5                                     |
| `R9`     | Argument 6                                     |

### Clobbered Registers
The `syscall` instruction clobbers:
- `RCX` - Saved user RIP
- `R11` - Saved user RFLAGS

### Return Values
- Success: Non-negative value (≥ 0)
- Error: Negative errno value (e.g., `-ENOSYS`)

### Syscall Entry Sequence
1. `syscall` instruction executes
2. Kernel swaps to kernel GS base (`swapgs`)
3. User RSP is saved to per-CPU scratch space
4. Kernel switches to kernel stack
5. Full register state is pushed (for fork support)
6. Rust syscall handler is invoked
7. Return value placed in RAX
8. Registers restored, `sysretq` returns to user mode

### Implemented Syscalls

| Number | Name      | Description                     |
|--------|-----------|---------------------------------|
| 0      | `read`    | Read from file descriptor       |
| 1      | `write`   | Write to file descriptor        |
| 2      | `open`    | Open a file                     |
| 3      | `close`   | Close file descriptor           |
| 8      | `lseek`   | Seek in file                    |
| 32     | `dup`     | Duplicate file descriptor       |
| 33     | `dup2`    | Duplicate fd to specific number |
| 39     | `getpid`  | Get process ID                  |
| 57     | `fork`    | Fork process                    |
| 59     | `execve`  | Execute program                 |
| 60     | `exit`    | Terminate process               |
| 61     | `waitpid` | Wait for child process          |
| 110    | `getppid` | Get parent process ID           |

---

## Process Execution Model

### Process States
- **Created**: Initial state before preparation
- **Ready**: Runnable, waiting for CPU
- **Running**: Currently executing on a CPU
- **Blocked**: Waiting on a lock or I/O
- **WaitingFor**: Waiting for specific process (waitpid)
- **Exited**: Terminated, waiting to be reaped

### Process Structure
Each process has:
- **PID**: Unique process identifier
- **CR3**: Page table physical address
- **ProcessState**: Saved CPU registers (RAX-R15, RIP, RFLAGS, RSP, CS, SS, FPU state)
- **File Handles**: HashMap of open file descriptors (0=stdin, 1=stdout, 2=stderr)
- **CWD**: Current working directory inode
- **Load Bias**: Base address for PIE binaries
- **Priority**: Scheduling priority (lower = higher priority)

### Address Space Isolation
- Each process has its own page tables (CR3)
- User space (PML4 entries 0-255) is process-private
- Kernel space (PML4 entries 256-511) is shared across all processes
- Copy-on-Write (COW) is used for `fork()` to share pages until modified

### ELF Loading
- ET_EXEC binaries are loaded at their specified addresses
- ET_DYN (PIE) binaries get a load bias of `0x400000` (4 MiB)
- LOAD segments are mapped with appropriate permissions (R/W/X)
- BSS sections are zero-initialized

---

## User Stack Layout

When a process starts, the stack is set up according to the System V AMD64 ABI:

```
High Address
┌─────────────────────────┐
│ Environment strings     │  (null-terminated)
│ Argument strings        │  (null-terminated)
├─────────────────────────┤
│ Padding for alignment   │
├─────────────────────────┤
│ AT_NULL (auxv end)      │  (0, 0)
├─────────────────────────┤
│ NULL (envp terminator)  │
│ envp[n-1]               │  → pointer to env string
│ ...                     │
│ envp[0]                 │
├─────────────────────────┤
│ NULL (argv terminator)  │
│ argv[argc-1]            │  → pointer to arg string
│ ...                     │
│ argv[0]                 │
├─────────────────────────┤
│ argc                    │  ← RSP points here
└─────────────────────────┘
Low Address
```

The stack pointer (RSP) must be 16-byte aligned before the `call main` instruction.

---

## C Runtime (crt0)

The `_start` entry point in `crt0.asm` performs:

1. Clear frame pointer (`xor rbp, rbp`)
2. Read `argc` from `[rsp]`
3. Calculate `argv` = `rsp + 8`
4. Calculate `envp` = `argv + (argc + 1) * 8`
5. Call `__libc_init(envp)` for libc initialization
6. Call `main(argc, argv)`
7. Call `exit(return_value)`

### Libc Syscall Interface

From C, syscalls are invoked via:
```c
long syscall(long num, long a1, long a2, long a3, long a4, long a5, long a6);
```

The implementation uses inline assembly to load registers and execute `syscall`.

---

## Context Switching

### Timer-Driven Preemption
1. Timer interrupt fires (IRQ0/APIC timer)
2. Assembly handler saves full register state
3. `SWAPGS` if coming from user mode
4. Rust scheduler determines next process
5. If switching: CR3 is changed, state is restored
6. `IRETQ` returns to (potentially different) process

### Saved Context (ProcessState)
```
Offset  Field
0x00    rax
0x08    rbx
0x10    rcx
0x18    rdx
0x20    rsi
0x28    rdi
0x30    rbp
0x38    rsp
0x40    r8
0x48    r9
0x50    r10
0x58    r11
0x60    r12
0x68    r13
0x70    r14
0x78    r15
0x80    rip
0x88    rflags
0x90    cs
0x98    ss
0xa0    fxsave area (512 bytes)
```

### Per-CPU Data
Accessed via GS segment (set up with `SWAPGS`):

| Offset | Field              | Description               |
|--------|--------------------|---------------------------|
| 0x00   | `cpu_id`           | CPU identifier            |
| 0x08   | `self_ptr`         | Pointer to this structure |
| 0x10   | `kernel_stack_ptr` | Kernel stack for this CPU |
| 0x18   | `scratch[0]`       | User RSP during syscall   |
| 0x20   | `scratch[1-3]`     | Additional scratch space  |
| 0x38   | `curr_pid`         | Currently running PID     |

---

## GDT Layout

| Index | Selector | Description                  |
|-------|----------|------------------------------|
| 0     | 0x00     | Null descriptor              |
| 1     | 0x08     | Kernel Code (64-bit, Ring 0) |
| 2     | 0x10     | Kernel Data (Ring 0)         |
| 3     | 0x18     | TSS (16-byte descriptor)     |
| 4     | -        | TSS (continued)              |
| 5     | 0x28     | User Data (Ring 3)           |
| 6     | 0x30     | User Code (64-bit, Ring 3)   |

For `sysret`, STAR MSR is configured with:
- Kernel CS at bits 32-47
- User base (Data - 8) at bits 48-63

`SYSRET` loads CS from `(STAR[48:63] + 16) | 3` and SS from `(STAR[48:63] + 8) | 3`.

---

## Copy-on-Write (COW)

Fork uses COW to efficiently share memory:

1. Parent and child share the same physical pages
2. Writable pages are marked read-only with a software COW flag (bit 9)
3. Reference counts track shared pages
4. On write fault:
   - If refcount == 1: Just make writable
   - If refcount > 1: Allocate new page, copy, decrement refcount
5. TLB is flushed after modification

---

## Error Codes

Standard POSIX-compatible errno values:

| Code | Name   | Description               |
|------|--------|---------------------------|
| 1    | EPERM  | Operation not permitted   |
| 2    | ENOENT | No such file or directory |
| 9    | EBADF  | Bad file descriptor       |
| 12   | ENOMEM | Out of memory             |
| 14   | EFAULT | Bad address               |
| 22   | EINVAL | Invalid argument          |
| 38   | ENOSYS | Function not implemented  |

---

## References

- [AMD64 Architecture Programmer's Manual](https://developer.amd.com/resources/developer-guides-manuals/)
- [System V AMD64 ABI](https://refspecs.linuxfoundation.org/elf/x86_64-abi-0.99.pdf)
- [OSDev Wiki](https://wiki.osdev.org/)
