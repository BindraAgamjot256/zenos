# Zenos Developer Documentation

> [!NOTE]
> This file is pure satire

Welcome to the trenches. This is the real shit that happens behind the scenes.

## Quick start

```bash
    git clone <repository-url>
    cd zenos
    rustup toolchain install nightly
    rustup component add rust-src llvm-tools-preview rustfmt clippy
    rustup target add x86_64-unknown-none
    cargo run
```

## What the fuck is this?

Zenos is an x86_64 operating system written in Rust because apparently writing an OS in C wasn't masochistic enough. I
decided to combine the pain of bare-metal programming with Rust's borrow checker yelling at us for trying to do
literally anything.

### Why Rust for an OS?

- Memory safety (allegedly)
- Zero-cost abstractions (narrator: there are costs)
- The masochistic joy of fighting the borrow checker while also fighting the CPU

## Architecture Deep Dive (aka "How I fucked this up")

### Memory Management—The Crown Jewel of Suffering

I have a two-tier memory management system because one layer of complexity wasn't enough:

1. **Page Allocator**: A bitmap-based allocator that uses linked lists stored in higher-half virtual addresses. Yes, I
   store the metadata in the memory I'me managing. It's turtles all the way down.

2. **Slab Allocator**: Because malloc() is for wimps, I implemented my own fixed-size block allocator. It has nine size
   classes from 8 bytes to 2KiB because fuck you, that's why.

The slab allocator version is literally `v0.0.sqrt(-1)-don't_you_dare_test_it_on_hardware`. That's not a joke, that's
the actual version string in the code comments.

### Process Management—Russian Roulette with Registers

We implemented preemptive multitasking because single-threaded execution was too stable.

- **Scheduler**: A round-robin scheduler that switches tasks whenever it feels like it (or when the timer interrupt
  fires).
- **Isolation**: We put users in Ring 3 so they can't hurt us. They still find ways.
- **Context Switching**: We save all the registers, swap the stack, and pray the new process knows what it's doing.

### System Calls—The Doorway to Hell

We use the `syscall` ~~`int 0x80`~~ instruction because software interrupts are so 1990s ~~syscalls are hard~~.

- **The Macro**: We have a `#[syscall]` macro that puts function pointers into a special linker section. It's basically
  linker magic abuse.
- **The Handler**: It takes values from registers, casts them to types that might be correct, and calls kernel
  functions. Security? What's that?

**Current syscall numbers** (Linux-compatible where possible):

| #   | Name    | What it does               |
|-----|---------|----------------------------|
| 0   | read    | Read from fd               |
| 1   | write   | Write to fd                |
| 2   | open    | Open a file                |
| 3   | close   | Close fd                   |
| 8   | lseek   | Seek in file               |
| 39  | getpid  | Get process ID             |
| 57  | fork    | Create child process       |
| 59  | execve  | Replace process            |
| 60  | exit    | Die gracefully             |
| 61  | wait    | Wait for child (hopefully) |
| 110 | getppid | Get parent process ID      |

### Build System - A Beautiful Disaster

Our build process is an unholy marriage of:

- Cargo workspaces (because one crate is never enough)
- Custom build scripts that coordinate between kernel and bootloader
- QEMU integration that ~~sometimes~~ always works
- A custom target specification because `x86_64-unknown-none` wasn't good enough

The build flow goes like this:

1. Build the kernel for a target that doesn't officially exist
2. Build a bootloader that packages said kernel
3. Create a disk image that UEFI can understand
4. Pray to whatever deity you believe in
5. Launch QEMU and hope it doesn't immediately crash

### Testing Framework—The Least Broken Part

Surprisingly, our testing framework is the most sane part of this entire project:

- Tests run in QEMU (because testing an OS on the host OS is... problematic)
- I have a custom test harness that actually works
- Serial output for debugging (because printf debugging is eternal)
- Tests can be conditionally compiled based on whether I hate myself today

### The Bootloader Situation

I forked the `bootloader` crate because apparently I needed to ~~make our lives even more complicated~~ get better logs.
The bootloader:

- Creates UEFI-compatible disk images
- Handles memory region discovery
- Sets up the initial memory mappings
- Passes control to the kernel while crossing fingers

## Development Workflow (aka "How to Suffer Productively")

### Setting Up Your Environment

1. Install Rust nightly because ~~stable is for cowards~~ stable is unsupported:
   ```bash
   rustup toolchain install nightly
   rustup component add rust-src llvm-tools-preview rustfmt clippy
   rustup target add x86_64-unknown-none
   ```

2. Install QEMU and prepare for pain:
   - Ubuntu/Debian: `sudo apt install qemu-system-x86`
   - macOS: `brew install qemu`
   - Windows: Download from qemu.org and add to PATH

### Common Commands That Might Work

```bash
# Build everything (and pray)
cargo build

# Run in QEMU
cargo run

# Check if code compiles without actually building (fast failure)
cargo check

# Make the code prettier (lipstick on a pig)
cargo fmt

# Let Clippy tell you everything wrong with your life choices
cargo clippy
```

### Debugging This Mess

When shit inevitably hits the fan:

1. **Serial Output**: Your best friend. Everything goes to UART, check your terminal
2. **Memory Dumps**: Use QEMU's monitor to inspect memory when things get weird
3. **GDB**: You can attach GDB ~~or LLDB~~ to QEMU if you're feeling masochistic
4. **kprint! Debugging**: Sometimes the old ways are the best ways

### Common Issues and How to Fix Them

**"It doesn't boot"**: Check that OVMF is properly installed and QEMU can find it.

**"Tests fail randomly"**: Welcome to bare-metal programming, where race conditions are everywhere and debugging is
hell.

**"Memory allocator panics"**: The slab allocator is held together with duct tape and hope. Check that page allocation
is working first.

**"ACPI parsing fails"**: Different machines have different ACPI tables. QEMU's are relatively sane, real hardware is
chaos. If anything bad happens, tell the ACPI crate.

**"Build fails with cryptic errors"**: Nightly Rust sometimes breaks. Pin to a known-good version or sacrifice a rubber
duck to the compiler gods.

## Code Organization (aka "Where Everything Lives")

### zenos-kernel/

The main event. Contains:

- `memory/`: Both allocators and all the pain they bring
- `process/`: The scheduler that decides who lives and who dies (CPU time-wise)
- `syscall/`: The gateway drug to kernel mode
- `disk/`: VFS, FAT filesystem, and procfs (yes, we have /proc now!)
- `tty/`: TTY subsystem with full ANSI escape sequence support and 256-color palette
- `framebuffer/`: Graphics because serial output is for peasants
- `interrupts/`: GDT and IDT setup (dragons be here)
- `hardware/`: ACPI parsing, PCI enumeration, and hardware abstraction
- `testing/`: The test framework that keeps us ~~in~~sane

### zenos-bootloader/

Our custom bootloader because apparently I hate myself:

- `api/`: Interface between bootloader and kernel
- `common/`: Shared code that both sides need
- `uefi/`: UEFI-specific implementation details

### zenos-shell/

An actual interactive shell! Contains:

- `main.c`: Shell entry point with ANSI color prompt
- `builtins.c`: Built-in commands (echo, help, clear, version, exit, getpid)
- `input.c`: Line reading and input handling
- `commands.c`: Command execution logic

Build with `cd zenos-shell && make`.

### zenos-stress-tests/

Stress tests for the kernel. Contains:

- `fork_storm.c`: Fork bomb to test process limits
- `fs_concurrent.c`: Concurrent filesystem access
- `mem_exhaust.c`: Memory exhaustion testing
- `orphan_zombie.c`: Orphan and zombie process handling
- `rapid_spawn.c`: Rapid process spawning
- `sched_fairness.c`: Scheduler fairness testing

### zenos-fuzz/

Fuzzing infrastructure for finding bugs in kernel subsystems.

### libc/

A minimal C library because we needed to run C code and didn't want to port glibc. Contains:

- `crt0.asm`: C runtime startup—entry point that sets up argc/argv/envp and calls main()
- `syscall.c`: Raw syscall wrapper using x86_64 `syscall` instruction
- `stdio.c`: printf() with %d, %u, %x, %p, %s, %c, %% (no buffering, writes directly to fd 1)
- `string.c`: The usual suspects—strlen, strcpy, memcpy, memset, etc.
- `unistd.c`: POSIX-ish wrappers—read, write, close, lseek, fork, execve, exit
- `fcntl.c`: open() with flags (O_RDONLY, O_WRONLY, O_CREAT, O_TRUNC)
- `math.c`: Software math functions (sin, cos, sqrt, exp, log, etc.)—no FPU required!

Build with `cd libc && make`. Link with `crt0.o` FIRST, then your objects, then `-lc`.

See [libc/README.md](libc/README.md) for the full API reference.

### syscall-macro/

Procedural macros that make the syscall table work. Contains high concentrations of `syn`, `quote` and black magic.

### Root Directory

- `src/main.rs`: QEMU launcher (literally just runs qemu-system-x86_64)
- `build.rs`: Build orchestration nightmare
- `Cargo.toml`: Workspace coordination
- `.cargo/config.toml`: Cargo configuration because defaults are never good enough

## Memory Layout (aka "Where Everything Goes Wrong")

> [!NOTE]
> You can view more info about the memory layout in `zenos-kernel/src/memory/mod.rs`.

- **0xFFFF_8000_0000_0000**: Higher-half base (physical memory offset)
- **0x4444_0000_0000**: Slab allocator base (arbitrary choice)
- **0x5555_0000_0000**: Large allocation base (not implemented yet)
- **Recursive mapping at P4[511]**: Because I needed more complexity

## Known Issues (aka "Features")

1. **No SMP support**: Single-core only because threading is hard
2. **Limited hardware support**: Works in QEMU, good luck on real hardware
3. **Memory leaks**: The slab allocator sometimes forgets to deallocate
4. **Race conditions**: Interrupts and memory allocation don't always play nice
5. ~~**Scheduler bias**: The scheduler might hate your process specifically~~
6. **Documentation lies**: Some comments are aspirational rather than factual

## Contributing (aka "Joining the Madness")

If you want to contribute to this beautiful disaster:

1. **Read the code**: It's simultaneously the best and worst documentation
2. **Test everything**: If it compiles, it might work. If it works, it might be correct.
3. **Add tests**: Future you will thank past you. ~~PS: Tests can be written by ChatGPT~~
4. **Document your crimes**: ~~LOL Maybe~~ Leave comments explaining why you did what you did(or ask chatgpt to do it
   for you).

### Code Style

- Use `rustfmt` religiously
- Clippy is your friend (and enemy)
- Comments should explain *why*, not *what*
- If you have to use `unsafe`, document the hell out of it
- Panic messages should be helpful (or at least entertaining... especially entertaining.)

## Performance Notes (aka "Why Is It So Slow?")

This is not a performance-focused OS. It's a learning project. That said:

- The page allocator is O(1) (when it works)
- The slab allocator is also O(1) (allegedly)
- Serial I/O is slow as molasses
- QEMU emulation adds overhead
- Debug builds are especially painful
- Performance is important if you are actually planning SMP.

## Future Plans (aka "Pipe Dreams")

Things I might implement if I ever finish what I started:

- Inode based VFS (because i want to cosplay as Unix)
- A real file system (FAT is just a placeholder)
- Pipes and IPC (because processes should talk to each other, even if they shouldn't)
- A shell that does more than just print "Hello, world!" (maybe a REPL for testing syscalls?)
- Every single syscall in Linux (because why not)
- A real scheduler with priorities and maybe even a multilevel feedback queue (Copilot said so, i have no clue what it means)
- Network stack (serial is good enough for now)
- SMP support (single-core is simpler)
- Real hardware support (QEMU is our friend)

## Final Notes

This project exists at the intersection of "educational" and "questionable life choices." If you're here to learn OS
development, welcome to the pain. If you're here to use this as a real OS, please reconsider your life decisions and
switch to the superior operating system(~~Linux~~ TempleOS).

The code quality varies from "not terrible" to "what was I thinking?" Comments like "FIX THE FUCKING TEST WILL YOU?" are
not bugs, they're features.

Remember: if it compiles, ship it. If it doesn't crash immediately, call it stable. If the tests pass, celebrate.

Good luck, and may the odds be ever in your favor.

> [!NOTE]
> This dev documentation is more fun to read than zenos itself.

---

*"zenos" is pronounced like Zeno's paradox, not "zen OS." Though given the amount of zen required to debug this thing,
the confusion is understandable.*

