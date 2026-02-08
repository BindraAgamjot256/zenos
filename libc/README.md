# Zenos libc

A minimal freestanding C standard library for Zenos userspace programs.

## Overview

This libc provides the essential C runtime and standard library functions needed to run C programs on Zenos. It's designed to be small, simple, and suitable for a bare-metal OS environment—no glibc, no musl, just the basics.

## What's Included

| Module      | Description                                                                 |
|-------------|-----------------------------------------------------------------------------|
| `crt0.asm`  | C runtime startup—calls `__libc_init(envp)` then `main(argc, argv)`         |
| `crt1.c`    | C runtime initialization—stores `environ`, provides `getenv()`              |
| `syscall.c` | Raw syscall interface using `syscall` instruction                           |
| `stdio.c`   | `printf` with basic format specifiers                                       |
| `string.c`  | String and memory manipulation functions                                    |
| `unistd.c`  | POSIX-like I/O and process control functions                                |
| `fcntl.c`   | File control: `open`, `openat`, `fcntl`, `mkdir`, etc.                      |
| `stat.c`    | File status: `stat`, `fstat`, `lstat`                                       |
| `stdlib.c`  | Utility functions: `atoi`, `atol`, `getenv`                                 |
| `errno.c`   | Error number support                                                        |
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
- `build/libc.a` — Static library (includes crt1.o and all other modules)

## Linking Your Program

```bash
# Compile your program
clang -ffreestanding -nostdlib -c myprogram.c -o myprogram.o -Ilibc/include

# Link with libc (crt0.o MUST come first!)
ld -o myprogram libc/build/crt0.o myprogram.o -Llibc/build -lc
```

---

## API Reference

### Environment Variables (`<stdlib.h>`)

| Function | Signature                        | Description                                                                                                    |
|----------|----------------------------------|----------------------------------------------------------------------------------------------------------------|
| `getenv` | `char *getenv(const char *name)` | Search the environment list for `name=value` and return a pointer to the value string, or `NULL` if not found. |

**Global Variables:**
- `extern char **environ` — Pointer to the environment array (set by `crt0` before `main`).

**Example:**
```c
#include <stdlib.h>
#include <stdio.h>

int main(int argc, char **argv) {
    char *path = getenv("PATH");
    if (path) {
        printf("PATH=%s\n", path);
    }
    return 0;
}
```

---

### Error Handling (`<errno.h>`)

| Variable/Function    | Description                                                   |
|----------------------|---------------------------------------------------------------|
| `errno`              | Global error number set by syscalls on failure.               |
| `__errno_location()` | Returns pointer to `errno` (for future thread-local support). |

**Common Error Codes:**

| Code | Name      | Description               |
|------|-----------|---------------------------|
| 1    | `EPERM`   | Operation not permitted   |
| 2    | `ENOENT`  | No such file or directory |
| 3    | `ESRCH`   | No such process           |
| 5    | `EIO`     | I/O error                 |
| 9    | `EBADF`   | Bad file descriptor       |
| 12   | `ENOMEM`  | Out of memory             |
| 13   | `EACCES`  | Permission denied         |
| 14   | `EFAULT`  | Bad address               |
| 17   | `EEXIST`  | File exists               |
| 20   | `ENOTDIR` | Not a directory           |
| 21   | `EISDIR`  | Is a directory            |
| 22   | `EINVAL`  | Invalid argument          |
| 38   | `ENOSYS`  | Function not implemented  |

**Example:**
```c
#include <fcntl.h>
#include <errno.h>
#include <stdio.h>

int main(void) {
    int fd = open("/nonexistent", O_RDONLY);
    if (fd < 0) {
        printf("Error: %d\n", errno);  // Prints ENOENT (2)
    }
    return 0;
}
```

---

### File I/O (`<unistd.h>`, `<fcntl.h>`)

#### Opening and Closing Files

| Function | Signature                                                     | Description                                                               |
|----------|---------------------------------------------------------------|---------------------------------------------------------------------------|
| `open`   | `int open(const char *pathname, int flags, ...)`              | Open a file. Returns file descriptor on success, -1 on error.             |
| `openat` | `int openat(int dirfd, const char *pathname, int flags, ...)` | Open file relative to directory fd. Use `AT_FDCWD` for current directory. |
| `creat`  | `int creat(const char *pathname, mode_t mode)`                | Create a file (equivalent to `open` with `O_CREAT                         |O_WRONLY|O_TRUNC`). |
| `close`  | `int close(int fd)`                                           | Close a file descriptor.                                                  |

**Open Flags (`<fcntl.h>`):**

| Flag         | Value    | Description                          |
|--------------|----------|--------------------------------------|
| `O_RDONLY`   | 0        | Open for reading only                |
| `O_WRONLY`   | 1        | Open for writing only                |
| `O_RDWR`     | 2        | Open for reading and writing         |
| `O_CREAT`    | 0100     | Create file if it doesn't exist      |
| `O_EXCL`     | 0200     | Fail if file exists (with `O_CREAT`) |
| `O_TRUNC`    | 01000    | Truncate file to zero length         |
| `O_APPEND`   | 02000    | Append writes to end of file         |
| `O_NONBLOCK` | 04000    | Non-blocking I/O                     |
| `O_CLOEXEC`  | 02000000 | Close on exec                        |

#### Reading and Writing

| Function | Signature                                                             | Description                                                                               |
|----------|-----------------------------------------------------------------------|-------------------------------------------------------------------------------------------|
| `read`   | `ssize_t read(int fd, void *buf, size_t count)`                       | Read up to `count` bytes from `fd` into `buf`. Returns bytes read, 0 on EOF, -1 on error. |
| `write`  | `ssize_t write(int fd, const void *buf, size_t count)`                | Write `count` bytes from `buf` to `fd`. Returns bytes written or -1 on error.             |
| `pread`  | `ssize_t pread(int fd, void *buf, size_t count, off_t offset)`        | Read at specific offset without changing file position.                                   |
| `pwrite` | `ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset)` | Write at specific offset without changing file position.                                  |
| `lseek`  | `off_t lseek(int fd, off_t offset, int whence)`                       | Reposition file offset. Returns new offset or -1 on error.                                |

**lseek `whence` values:**

| Constant   | Value | Description                   |
|------------|-------|-------------------------------|
| `SEEK_SET` | 0     | Offset from beginning of file |
| `SEEK_CUR` | 1     | Offset from current position  |
| `SEEK_END` | 2     | Offset from end of file       |

#### File Descriptor Manipulation

| Function | Signature                                   | Description                                                       |
|----------|---------------------------------------------|-------------------------------------------------------------------|
| `dup`    | `int dup(int oldfd)`                        | Duplicate file descriptor, returning lowest available fd.         |
| `dup2`   | `int dup2(int oldfd, int newfd)`            | Duplicate `oldfd` to `newfd`, closing `newfd` first if open.      |
| `dup3`   | `int dup3(int oldfd, int newfd, int flags)` | Like `dup2` but with flags (e.g., `O_CLOEXEC`).                   |
| `pipe`   | `int pipe(int pipefd[2])`                   | Create a pipe. `pipefd[0]` is read end, `pipefd[1]` is write end. |
| `pipe2`  | `int pipe2(int pipefd[2], int flags)`       | Like `pipe` but with flags.                                       |
| `fcntl`  | `int fcntl(int fd, int cmd, ...)`           | File control operations (get/set flags, duplicate fd).            |

**fcntl commands:**

| Command   | Description                             |
|-----------|-----------------------------------------|
| `F_DUPFD` | Duplicate fd to lowest available >= arg |
| `F_GETFD` | Get fd flags                            |
| `F_SETFD` | Set fd flags                            |
| `F_GETFL` | Get file status flags                   |
| `F_SETFL` | Set file status flags                   |

---

### Process Control (`<unistd.h>`)

| Function  | Signature                                                                  | Description                                                                   |
|-----------|----------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `fork`    | `pid_t fork(void)`                                                         | Create child process. Returns 0 in child, child's PID in parent, -1 on error. |
| `execve`  | `int execve(const char *pathname, char *const argv[], char *const envp[])` | Replace current process with new program. Only returns on error (-1).         |
| `execv`   | `int execv(const char *pathname, char *const argv[])`                      | Like `execve` but uses current environment (`environ`).                       |
| `execvp`  | `int execvp(const char *file, char *const argv[])`                         | Like `execv` but searches `PATH` for executable.                              |
| `execvpe` | `int execvpe(const char *file, char *const argv[], char *const envp[])`    | Like `execvp` but with custom environment.                                    |
| `_exit`   | `void _exit(int status)`                                                   | Terminate process immediately (no cleanup).                                   |
| `exit`    | `void exit(int status)`                                                    | Terminate process (currently same as `_exit`).                                |

**Example:**
```c
#include <unistd.h>
#include <stdio.h>

int main(void) {
    pid_t pid = fork();
    if (pid == 0) {
        // Child process
        char *args[] = {"/bin/echo", "Hello from child!", NULL};
        execv("/bin/echo", args);
        _exit(1);  // Only reached if execv fails
    } else if (pid > 0) {
        // Parent process
        printf("Spawned child PID %d\n", pid);
        waitpid(pid, NULL, 0);
    }
    return 0;
}
```

---

### Process Information (`<unistd.h>`)

| Function  | Signature               | Description                                |
|-----------|-------------------------|--------------------------------------------|
| `getpid`  | `pid_t getpid(void)`    | Get current process ID.                    |
| `getppid` | `pid_t getppid(void)`   | Get parent process ID.                     |
| `getpgrp` | `pid_t getpgrp(void)`   | Get process group ID.                      |
| `setsid`  | `pid_t setsid(void)`    | Create new session, become session leader. |
| `getuid`  | `uid_t getuid(void)`    | Get real user ID.                          |
| `geteuid` | `uid_t geteuid(void)`   | Get effective user ID.                     |
| `getgid`  | `gid_t getgid(void)`    | Get real group ID.                         |
| `getegid` | `gid_t getegid(void)`   | Get effective group ID.                    |
| `setuid`  | `int setuid(uid_t uid)` | Set user ID.                               |
| `setgid`  | `int setgid(gid_t gid)` | Set group ID.                              |

---

### Waiting for Processes (`<unistd.h>`, `<sys/wait.h>`)

| Function  | Signature                                            | Description                              |
|-----------|------------------------------------------------------|------------------------------------------|
| `wait`    | `pid_t wait(int *status)`                            | Wait for any child process to terminate. |
| `waitpid` | `pid_t waitpid(pid_t pid, int *status, int options)` | Wait for specific child process.         |

**waitpid `pid` values:**

| Value | Description                              |
|-------|------------------------------------------|
| `> 0` | Wait for child with this PID             |
| `-1`  | Wait for any child                       |
| `0`   | Wait for any child in same process group |

**Status macros (`<sys/wait.h>`):**

| Macro                 | Description                      |
|-----------------------|----------------------------------|
| `WIFEXITED(status)`   | True if child exited normally    |
| `WEXITSTATUS(status)` | Exit code (if `WIFEXITED`)       |
| `WIFSIGNALED(status)` | True if child killed by signal   |
| `WTERMSIG(status)`    | Signal number (if `WIFSIGNALED`) |

---

### File System Operations (`<unistd.h>`, `<fcntl.h>`)

| Function    | Signature                                                                            | Description                                 |
|-------------|--------------------------------------------------------------------------------------|---------------------------------------------|
| `access`    | `int access(const char *pathname, int mode)`                                         | Check file accessibility.                   |
| `faccessat` | `int faccessat(int dirfd, const char *pathname, int mode, int flags)`                | Like `access` but relative to directory fd. |
| `chdir`     | `int chdir(const char *path)`                                                        | Change current working directory.           |
| `getcwd`    | `char *getcwd(char *buf, size_t size)`                                               | Get current working directory.              |
| `mkdir`     | `int mkdir(const char *pathname, mode_t mode)`                                       | Create a directory.                         |
| `mkdirat`   | `int mkdirat(int dirfd, const char *pathname, mode_t mode)`                          | Create directory relative to fd.            |
| `rmdir`     | `int rmdir(const char *pathname)`                                                    | Remove an empty directory.                  |
| `unlink`    | `int unlink(const char *pathname)`                                                   | Delete a file.                              |
| `unlinkat`  | `int unlinkat(int dirfd, const char *pathname, int flags)`                           | Delete file relative to fd.                 |
| `rename`    | `int rename(const char *oldpath, const char *newpath)`                               | Rename a file.                              |
| `renameat`  | `int renameat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath)` | Rename relative to directory fds.           |
| `readlink`  | `ssize_t readlink(const char *pathname, char *buf, size_t bufsiz)`                   | Read symbolic link target.                  |
| `chmod`     | `int chmod(const char *pathname, mode_t mode)`                                       | Change file permissions.                    |
| `chown`     | `int chown(const char *pathname, uid_t owner, gid_t group)`                          | Change file ownership.                      |
| `fchownat`  | `int fchownat(int dirfd, const char *pathname, uid_t owner, gid_t group, int flags)` | Change ownership relative to fd.            |

**access `mode` flags:**

| Flag   | Value | Description              |
|--------|-------|--------------------------|
| `F_OK` | 0     | Check file exists        |
| `R_OK` | 4     | Check read permission    |
| `W_OK` | 2     | Check write permission   |
| `X_OK` | 1     | Check execute permission |

---

### File Status (`<sys/stat.h>`)

| Function | Signature                                               | Description                              |
|----------|---------------------------------------------------------|------------------------------------------|
| `stat`   | `int stat(const char *pathname, struct stat *statbuf)`  | Get file status by path.                 |
| `fstat`  | `int fstat(int fd, struct stat *statbuf)`               | Get file status by file descriptor.      |
| `lstat`  | `int lstat(const char *pathname, struct stat *statbuf)` | Like `stat` but doesn't follow symlinks. |

**`struct stat` fields:**

| Field      | Type      | Description               |
|------------|-----------|---------------------------|
| `st_dev`   | `dev_t`   | Device ID                 |
| `st_ino`   | `ino_t`   | Inode number              |
| `st_mode`  | `mode_t`  | File type and permissions |
| `st_nlink` | `nlink_t` | Number of hard links      |
| `st_uid`   | `uid_t`   | Owner user ID             |
| `st_gid`   | `gid_t`   | Owner group ID            |
| `st_size`  | `off_t`   | File size in bytes        |
| `st_atime` | `time_t`  | Last access time          |
| `st_mtime` | `time_t`  | Last modification time    |
| `st_ctime` | `time_t`  | Last status change time   |

**File type test macros:**

| Macro         | Description          |
|---------------|----------------------|
| `S_ISREG(m)`  | Is regular file?     |
| `S_ISDIR(m)`  | Is directory?        |
| `S_ISCHR(m)`  | Is character device? |
| `S_ISBLK(m)`  | Is block device?     |
| `S_ISFIFO(m)` | Is FIFO (pipe)?      |
| `S_ISLNK(m)`  | Is symbolic link?    |
| `S_ISSOCK(m)` | Is socket?           |

---

### Signals and Timing (`<unistd.h>`)

| Function | Signature                                  | Description                                           |
|----------|--------------------------------------------|-------------------------------------------------------|
| `pause`  | `int pause(void)`                          | Suspend until signal received.                        |
| `sleep`  | `unsigned int sleep(unsigned int seconds)` | Sleep for specified seconds (currently uses `pause`). |
| `alarm`  | `unsigned int alarm(unsigned int seconds)` | Set alarm timer (stub, not implemented).              |

---

### Terminal (`<unistd.h>`)

| Function  | Signature               | Description                                    |
|-----------|-------------------------|------------------------------------------------|
| `isatty`  | `int isatty(int fd)`    | Check if fd is a terminal (always returns 1).  |
| `ttyname` | `char *ttyname(int fd)` | Get terminal name (always returns "/dev/tty"). |

---

### String Functions (`<string.h>`)

| Function  | Signature                                                | Description                                                 |
|-----------|----------------------------------------------------------|-------------------------------------------------------------|
| `strlen`  | `size_t strlen(const char *s)`                           | Calculate length of string (not including null terminator). |
| `strcpy`  | `char *strcpy(char *dest, const char *src)`              | Copy string `src` to `dest`.                                |
| `strncpy` | `char *strncpy(char *dest, const char *src, size_t n)`   | Copy at most `n` bytes, zero-padding if shorter.            |
| `strcmp`  | `int strcmp(const char *s1, const char *s2)`             | Compare two strings. Returns <0, 0, or >0.                  |
| `strncmp` | `int strncmp(const char *s1, const char *s2, size_t n)`  | Compare at most `n` bytes of two strings.                   |
| `strcat`  | `char *strcat(char *dest, const char *src)`              | Append `src` to end of `dest`.                              |
| `strncat` | `char *strncat(char *dest, const char *src, size_t n)`   | Append at most `n` bytes from `src`.                        |
| `strchr`  | `char *strchr(const char *s, int c)`                     | Find first occurrence of character `c`.                     |
| `strrchr` | `char *strrchr(const char *s, int c)`                    | Find last occurrence of character `c`.                      |
| `strstr`  | `char *strstr(const char *haystack, const char *needle)` | Find substring.                                             |
| `memset`  | `void *memset(void *s, int c, size_t n)`                 | Fill memory with byte value.                                |
| `memcpy`  | `void *memcpy(void *dest, const void *src, size_t n)`    | Copy memory (undefined for overlapping).                    |
| `memmove` | `void *memmove(void *dest, const void *src, size_t n)`   | Copy memory (safe for overlapping).                         |
| `memcmp`  | `int memcmp(const void *s1, const void *s2, size_t n)`   | Compare memory.                                             |

---

### Standard I/O (`<stdio.h>`)

| Function | Signature                             | Description                                        |
|----------|---------------------------------------|----------------------------------------------------|
| `printf` | `int printf(const char *format, ...)` | Formatted output to stdout. Returns bytes written. |

**printf Format Specifiers:**

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

---

### Utility Functions (`<stdlib.h>`)

| Function | Signature                    | Description                    |
|----------|------------------------------|--------------------------------|
| `atoi`   | `int atoi(const char *str)`  | Convert string to integer.     |
| `atol`   | `long atol(const char *str)` | Convert string to long.        |
| `_Exit`  | `void _Exit(int status)`     | Immediate process termination. |

**Memory allocation (stubs, not implemented):**

| Function  | Status       |
|-----------|--------------|
| `malloc`  | Returns NULL |
| `free`    | No-op        |
| `calloc`  | Returns NULL |
| `realloc` | Returns NULL |

---

### Low-Level Syscall Interface (`<sys/syscall.h>`)

| Function   | Signature                                          | Description                            |
|------------|----------------------------------------------------|----------------------------------------|
| `syscall`  | `long syscall(long n, long a1, ..., long a6)`      | Invoke syscall with up to 6 arguments. |
| `syscall0` | `long syscall0(long n)`                            | Invoke syscall with 0 arguments.       |
| `syscall1` | `long syscall1(long n, long a1)`                   | Invoke syscall with 1 argument.        |
| `syscall2` | `long syscall2(long n, long a1, long a2)`          | Invoke syscall with 2 arguments.       |
| `syscall3` | `long syscall3(long n, long a1, long a2, long a3)` | Invoke syscall with 3 arguments.       |
| `syscall4` | `long syscall4(long n, ...)`                       | Invoke syscall with 4 arguments.       |
| `syscall5` | `long syscall5(long n, ...)`                       | Invoke syscall with 5 arguments.       |
| `syscall6` | `long syscall6(long n, ...)`                       | Invoke syscall with 6 arguments.       |

---

## Supported Syscalls

The library wraps these Zenos syscalls:

| Number                      | Name         | Status      | Function                               |
|-----------------------------|--------------|-------------|----------------------------------------|
| 0                           | `read`       | Implemented | Read from file descriptor              |
| 1                           | `write`      | Implemented | Write to file descriptor               |
| 2                           | `open`       | Implemented | Open a file                            |
| 3                           | `close`      | Implemented | Close a file descriptor                |
| 4                           | `stat`       | Stub        | Get file status                        |
| 5                           | `fstat`      | Stub        | Get file status by fd                  |
| 6                           | `lstat`      | Stub        | Get file status (no symlink follow)    |
| 8                           | `lseek`      | Implemented | Seek in a file                         |
| 21                          | `access`     | Stub        | Check file accessibility               |
| 22                          | `pipe`       | Stub        | Create pipe                            |
| 32                          | `dup`        | Stub        | Duplicate file descriptor              |
| 33                          | `dup2`       | Stub        | Duplicate to specific fd               |
| 34                          | `pause`      | Stub        | Wait for signal                        |
| 39                          | `getpid`     | Implemented | Get process ID                         |
| 57                          | `fork`       | Implemented | Create child process                   |
| 59                          | `execve`     | Implemented | Execute a program                      |
| 60                          | `exit`       | Implemented | Terminate process                      |
| 61                          | `waitpid`    | Implemented | Wait for child                         |
| 72                          | `fcntl`      | Stub        | File control                           |
| 79                          | `getcwd`     | Stub        | Get current directory                  |
| 80                          | `chdir`      | Stub        | Change directory                       |
| 83                          | `mkdir`      | Stub        | Create directory                       |
| 84                          | `rmdir`      | Stub        | Remove directory                       |
| 87                          | `unlink`     | Stub        | Delete file                            |
| 90                          | `chmod`      | Stub        | Change file mode                       |
| 92                          | `chown`      | Stub        | Change file owner                      |
| 102                         | `getuid`     | Stub        | Get user ID                            |
| 104                         | `getgid`     | Stub        | Get group ID                           |
| 110                         | `getppid`    | Implemented | Get parent PID                         |
| 257                         | `openat`     | Stub        | Open relative to dir fd                |
| 258                         | `mkdirat`    | Stub        | Create dir relative to fd              |
| 263                         | `unlinkat`   | Stub        | Delete relative to fd                  |
| 264                         | `renameat`   | Stub        | Rename relative to fd                  |

---

## Math Library

All math functions are implemented in software using:
- **Taylor series** for trig and exponential functions
- **Newton-Raphson** for `sqrt` and `cbrt`
- **IEEE 754 bit manipulation** for `fabs`, `copysign`, `ldexp`, `frexp`

No hardware FPU instructions required. Both `double` and `float` variants provided.

---

## Limitations

- **No malloc/free** — Memory allocation returns NULL (implement your own)
- **No stdin buffering** — `printf` writes directly via syscall
- **No threads** — Single-threaded only (errno is global, not thread-local)
- **x86_64 only** — Uses inline assembly for syscalls

---

## Files

```
libc/
├── Makefile              # Build system
├── README.md             # This file
├── include/
│   ├── errno.h           # Error codes
│   ├── fcntl.h           # File control definitions
│   ├── math.h            # Math function declarations
│   ├── stdarg.h          # Variadic argument macros
│   ├── stddef.h          # Standard definitions (NULL, size_t)
│   ├── stdint.h          # Fixed-width integer types
│   ├── stdio.h           # printf declaration
│   ├── stdlib.h          # Utility functions, getenv
│   ├── string.h          # String function declarations
│   ├── unistd.h          # POSIX-like I/O declarations
│   └── sys/
│       ├── stat.h        # File status structures
│       ├── syscall.h     # Syscall numbers and wrappers
│       ├── types.h       # Type definitions (pid_t, uid_t, etc.)
│       └── wait.h        # Wait macros
└── src/
    ├── crt0.asm          # Entry point (_start → __libc_init → main)
    ├── crt1.c            # Runtime init (environ, getenv, later heap also)
    ├── errno.c           # errno variable
    ├── fcntl.c           # File operations
    ├── math.c            # Math implementations
    ├── stat.c            # stat/fstat/lstat
    ├── stdio.c           # printf()
    ├── stdlib.c          # atoi, atol
    ├── string.c          # String/memory functions
    ├── syscall.c         # Raw syscall interface
    └── unistd.c          # Process and I/O functions
```

---

## Example

```c
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

int main(int argc, char **argv) {
    printf("Hello from Zenos!\n");
    printf("PID: %d, PPID: %d\n", getpid(), getppid());
    
    // Environment variables
    char *path = getenv("PATH");
    printf("PATH=%s\n", path ? path : "(not set)");
    
    // File operations
    int fd = open("/etc/motd", O_RDONLY);
    if (fd < 0) {
        printf("open failed: errno=%d\n", errno);
    } else {
        char buf[256];
        ssize_t n = read(fd, buf, sizeof(buf) - 1);
        if (n > 0) {
            buf[n] = '\0';
            printf("Contents: %s\n", buf);
        }
        close(fd);
    }
    
    // Process creation
    pid_t pid = fork();
    if (pid == 0) {
        printf("I'm the child!\n");
        _exit(0);
    } else {
        waitpid(pid, NULL, 0);
        printf("Child exited\n");
    }
    
    return 0;
}
```
