# Zenos libc

A minimal freestanding C standard library for Zenos userspace programs.

## Overview

This libc provides the essential C runtime and standard library functions needed to run C programs on Zenos. It's designed to be small, simple, and suitable for a bare-metal OS environment—no glibc, no musl, just the basics.

## What's Included

| Module      | Description                                                                 |
|-------------|-----------------------------------------------------------------------------|
| `crt0.asm`  | C runtime startup—sets up `main(argc, argv, envp)`                          |
| `syscall.c` | Raw syscall interface using `syscall` instruction                           |
| `stdio.c`   | `printf` with basic format specifiers                                       |
| `string.c`  | String and memory manipulation functions                                    |
| `unistd.c`  | POSIX-like I/O: `read`, `write`, `close`, `lseek`, `fork`, `execve`, `exit` |
| `fcntl.c`   | File control: `open` with flags                                             |
| `math.c`    | Software-implemented math functions (no FPU required)                       |

## Building

```bash
# Build with clang (default)
make

# Build with gcc
make CC=gcc

# Debug build with symbols
make DEBUG=1

# Cross-compile for x86_64 from another host
make CC="clang --target=x86_64-unknown-none-elf"

# Clean
make clean
```

**Output:**
- `build/crt0.o` — C runtime startup object (link this FIRST!)
- `build/libc.a` — Static library

## Linking Your Program

```bash
# Compile your program
clang -ffreestanding -nostdlib -c myprogram.c -o myprogram.o -Ilibc/include

# Link with libc (crt0.o MUST come first!)
ld -o myprogram libc/build/crt0.o myprogram.o -Llibc/build -lc
```

## Supported Syscalls

The library wraps these Zenos syscalls:

| Number | Name     | Function                    |
|--------|----------|-----------------------------|
| 0      | `read`   | Read from file descriptor   |
| 1      | `write`  | Write to file descriptor    |
| 2      | `open`   | Open a file                 |
| 3      | `close`  | Close a file descriptor     |
| 8      | `lseek`  | Seek in a file              |
| 34     | `pause`  | Wait for signal             |
| 57     | `fork`   | Create child process        |
| 59     | `execve` | Execute a program           |
| 60     | `exit`   | Terminate process           |

## printf Format Specifiers

| Specifier | Description             | Example  |
|-----------|-------------------------|----------|
| `%d`      | Signed decimal int      | `-42`    |
| `%u`      | Unsigned decimal int    | `42`     |
| `%x`      | Hexadecimal (lowercase) | `2a`     |
| `%p`      | Pointer                 | `0x1234` |
| `%s`      | String                  | `hello`  |
| `%c`      | Character               | `A`      |
| `%%`      | Literal percent         | `%`      |

Width and zero-padding supported: `%08x` → `0000002a`

## Math Library

All math functions are implemented in software using:
- **Taylor series** for trig and exponential functions
- **Newton-Raphson** for `sqrt` and `cbrt`
- **IEEE 754 bit manipulation** for `fabs`, `copysign`, `ldexp`, `frexp`

No hardware FPU instructions required. Both `double` and `float` variants provided.

## Limitations

- **No malloc/free** — You'll need to implement your own or use static allocation
- **No stdin buffering** — `printf` writes directly via syscall
- **No signals** — `pause` exists but signal handling is not implemented
- **No threads** — Single-threaded only
- **x86_64 only** — Uses inline assembly for syscalls

## Files

```
libc/
├── Makefile              # Build system
├── README.md             # This file
├── include/
│   ├── fcntl.h           # File control definitions
│   ├── math.h            # Math function declarations
│   ├── stdarg.h          # Variadic argument macros
│   ├── stddef.h          # Standard definitions (NULL, size_t)
│   ├── stdint.h          # Fixed-width integer types
│   ├── stdio.h           # printf declaration
│   ├── string.h          # String function declarations
│   ├── unistd.h          # POSIX-like I/O declarations
│   └── sys/
│       ├── syscall.h     # Syscall numbers and wrappers
│       └── types.h       # Type definitions (ssize_t, off_t)
└── src/
    ├── crt0.asm          # Entry point (_start → main)
    ├── fcntl.c           # open()
    ├── math.c            # Math implementations
    ├── stdio.c           # printf()
    ├── string.c          # String/memory functions
    ├── syscall.c         # Raw syscall interface
    └── unistd.c          # read, write, close, etc.
```

## Example

```c
#include <stdio.h>
#include <unistd.h>

int main(int argc, char **argv, char **envp) {
    printf("Hello from Zenos!\n");
    printf("argc = %d\n", argc);
    
    for (int i = 0; i < argc; i++) {
        printf("argv[%d] = %s\n", i, argv[i]);
    }
    
    return 0;
}
```
