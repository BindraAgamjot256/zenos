# Zenos

An experimental x86_64 operating system written in Rust.

> **Note on pronunciation**: "Zenos" is pronounced /ˈziː.nɒsss/ (like "Zeno's paradox" with emphasis on the 's'), not
> as "zen OS."

## Overview

Zenos is a bare-metal operating system that demonstrates modern OS development techniques using Rust's memory safety and
zero-cost abstractions. The project includes a custom UEFI bootloader, kernel, and a minimal C standard library for userspace programs.

## Features

- **Memory Management**: Custom page and slab allocators with O(1) allocation
- **Process Management**: Process isolation with Ring 3 userspace execution
- **File System**: Virtual file system with support for file handles and standard I/O
- **System Calls**: Standardized syscall interface for user-space interaction
- **Graphics**: Framebuffer-based graphics output using embedded-graphics
- **Hardware Support**: ACPI parsing, APIC initialization, PCI enumeration, and UART serial I/O
- **C Library**: Minimal freestanding libc for userspace C programs
- **Testing**: Comprehensive test suite that runs in QEMU
- **Modular Design**: Clean separation between bootloader, kernel, and userspace components

## Quick Start

### Prerequisites

- Rust nightly toolchain with required components
- QEMU (`qemu-system-x86_64`) for emulation
- NASM assembler (for libc's crt0)

### Installation

1. Clone the repository:
   ```bash
   git clone <repository-url>
   cd zenos
   ```

2. Install Rust nightly with required components:
   ```bash
   rustup toolchain install nightly
   rustup component add rust-src llvm-tools-preview rustfmt clippy
   rustup target add x86_64-unknown-none
   ```

3. Install QEMU:
   - **Ubuntu/Debian**: `sudo apt install qemu-system-x86`
   - **macOS**: `brew install qemu`
   - **Windows**: Download from [qemu.org](https://www.qemu.org/download/)

### Running

Build and run the OS in QEMU:

```bash
cargo run
```

This will automatically:

1. Build the kernel and bootloader
2. Create a bootable disk image
3. Launch QEMU with the appropriate configuration

### Testing

Run the kernel test suite:

```bash
cargo run -- --test # currently not implemented, but this is how it will work. for now, test output is shown when cargo run is invoked, as soon as bootloader hands over to kernel.
```

Tests execute in a QEMU environment and verify kernel functionality.

## Project Structure

```
zenos/
├── src/                    # QEMU runner
├── zenos-init/             # Initial post-kernel setup
├── zenos-kernel/           # Main kernel implementation
│   └── src/
│       ├── memory/         # Memory management subsystem
│       ├── process/        # Process scheduler and isolation
│       ├── syscall/        # System call handlers
│       ├── fs/             # Virtual file system
│       ├── framebuffer/    # Graphics output
│       ├── interrupts/     # Interrupt handling
│       └── hardware/       # Hardware abstraction
├── zenos-bootloader/       # Custom UEFI bootloader
│   ├── api/               # Bootloader API
│   ├── common/            # Shared utilities
│   └── uefi/              # UEFI implementation
├── libc/                   # Minimal C standard library for userspace
│   ├── include/           # Header files (stdio.h, string.h, etc.)
│   └── src/               # Implementation (printf, syscalls, math, etc.)
├── syscall-macro/          # System call definition macros
├── iso/                    # Bootable disk image assets
└── build.rs               # Build orchestration
```

## Architecture

### Kernel Design

- **No Standard Library**: Runs in a `no_std` environment with custom allocators
- **Memory Safety**: Leverages Rust's ownership system for safer low-level programming
- **Higher-Half Kernel**: Uses virtual memory mapping at high addresses
- **Interrupt-Safe**: Careful interrupt management throughout the codebase

### Memory Management

- **Page Allocator**: Bitmap-based allocator for 4KiB and 2MiB pages
- **Slab Allocator**: Fixed-size block allocator with 9 size classes (8B to 2KiB)
- **Virtual Memory**: Recursive page table mapping with ACPI-discovered memory regions

### Process Management

- **Isolation**: Ring 3 user-space execution with separate page tables
- **Scheduling**: Basic round-robin scheduler for concurrent execution
- **IPC**: System call interface for kernel services

### Userspace (libc)

The `libc/` directory contains a minimal C standard library for writing userspace programs:

- **crt0.asm**: C runtime startup (sets up argc/argv/envp and calls main)
- **syscall.c**: Raw syscall interface using the x86_64 `syscall` instruction
- **stdio.c**: printf with basic format specifiers
- **string.c**: String and memory functions (strlen, memcpy, etc.)
- **unistd.c**: POSIX-like I/O (read, write, fork, execve, exit)
- **math.c**: Software-implemented math functions

See [libc/README.md](libc/README.md) for details.

### Hardware Support

- **UEFI Bootloader**: Custom bootloader supporting modern UEFI systems
- **ACPI Integration**: Hardware discovery through ACPI table parsing
- **PCI Support**: Device enumeration and configuration
- **Serial Output**: UART-based debugging and logging
- **Framebuffer Graphics**: Basic graphics output for visual feedback

## Development

### Building Components

```bash
# Build everything
cargo build

# Build specific components
cargo build -p zenos-kernel

# Release build
cargo build --release

# Build libc
cd libc && make
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy
```

## Contributing

This is an experimental project primarily for learning and demonstration purposes. Contributions are welcome,
particularly:

- Additional hardware support
- Improved memory management algorithms
- Enhanced testing coverage
- Documentation improvements

## License

MIT License—see LICENSE file for details.

## Acknowledgments

- Built with the [bootloader](https://github.com/rust-osdev/bootloader) ecosystem, forked in our
  own [zenos_bootloader](./zenos-bootloader) directory
- Uses [x86_64](https://github.com/rust-osdev/x86_64) for low-level hardware access
- Inspired by the [Writing an OS in Rust](https://os.phil-opp.com/) blog series
