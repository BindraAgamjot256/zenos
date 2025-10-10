# Zenos

An experimental x86_64 operating system written in Rust.

> **Note on pronunciation**: "Zenos" is pronounced /ˈziː.nɒsss/ (like "Zeno's paradox" with emphasis on the 's'), not as "zen OS".

## Overview

Zenos is a bare-metal operating system that demonstrates modern OS development techniques using Rust's memory safety and zero-cost abstractions. The project includes a custom UEFI bootloader and implements core kernel functionality including memory management, interrupt handling, and hardware abstraction.

## Features

- **Memory Management**: Custom page and slab allocators with O(1) allocation
- **Graphics**: Framebuffer-based graphics output using embedded-graphics
- **Hardware Support**: ACPI parsing, APIC initialization, and UART serial I/O
- **Testing**: Comprehensive test suite that runs in QEMU
- **Modular Design**: Clean separation between bootloader and kernel components

## Quick Start

### Prerequisites

- Rust nightly toolchain with required components
- QEMU (`qemu-system-x86_64`) for emulation

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
├── zenos-kernel/           # Main kernel implementation
│   └── src/
│       ├── memory/         # Memory management subsystem
│       ├── framebuffer/    # Graphics output
│       ├── interrupts/     # Interrupt handling
│       └── hardware/       # Hardware abstraction
├── zenos-bootloader/       # Custom UEFI bootloader
│   ├── api/               # Bootloader API
│   ├── common/            # Shared utilities
│   └── uefi/              # UEFI implementation
└── build.rs               # Build orchestration
```

## Architecture

### Kernel Design

- **No Standard Library**: Runs in a `no_std` environment with custom allocators
- **Memory Safety**: Leverages Rust's ownership system for safe~~er~~ low-level programming
- **Higher-Half Kernel**: Uses virtual memory mapping at high addresses
- **Interrupt-Safe**: Careful interrupt management throughout the codebase

### Memory Management

- **Page Allocator**: Bitmap-based allocator for 4KiB and 2MiB pages
- **Slab Allocator**: Fixed-size block allocator with 9 size classes (8B to 2KiB)
- **Virtual Memory**: Recursive page table mapping with ACPI-discovered memory regions

### Hardware Support

- **UEFI Bootloader**: Custom bootloader supporting modern UEFI systems
- **ACPI Integration**: Hardware discovery through ACPI table parsing
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
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy
```

## Contributing

This is an experimental project primarily for learning and demonstration purposes. Contributions are welcome, particularly:

- Additional hardware support
- Improved memory management algorithms
- Enhanced testing coverage
- Documentation improvements

## License

MIT License—see LICENSE file for details.

## Acknowledgments

- Built with the [bootloader](https://github.com/rust-osdev/bootloader) ecosystem, forked in our own [zenos_bootloader](./zenos_bootloader/Cargo.toml) directory
- Uses [x86_64](https://github.com/rust-osdev/x86_64) for low-level hardware access
- Inspired by the [Writing an OS in Rust](https://os.phil-opp.com/) blog series
