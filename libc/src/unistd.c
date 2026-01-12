/**
 * unistd.c - POSIX-like system call wrappers
 *
 * Provides familiar POSIX I/O and process functions by wrapping
 * the underlying Zenos syscalls.
 */

#include "unistd.h"
#include "sys/syscall.h"


/** Read up to count bytes from fd into buf, returns bytes read or -1 */
ssize_t read(const int fd, void *buf, const size_t count) {
    return syscall3(SYS_read, fd, (long) buf, (long) count);
}

/** Write count bytes from buf to fd, returns bytes written or -1 */
ssize_t write(const int fd, const void *buf, const size_t count) {
    return syscall3(SYS_write, fd, (long) buf, (long) count);
}

/** Close file descriptor */
int close(int fd) {
    return (int) syscall1(SYS_close, fd);
}

/** Reposition file offset (whence: SEEK_SET=0, SEEK_CUR=1, SEEK_END=2) */
off_t lseek(const int fd, const off_t offset, const int whence) {
    return syscall3(SYS_lseek, fd, offset, whence);
}

/** Create child process, returns 0 in child, child PID in parent, -1 on error */
int fork(void) {
    return (int) syscall0(SYS_fork);
}

/** Suspend process until signal (not fully implemented in Zenos) */
int pause(void) {
    return (int) syscall0(SYS_pause);
}
/** Replace current process with new program */
int execve(const char *filename, char *const argv[], char *const envp[]) {
    return (int) syscall3(SYS_execve, (long) filename, (long) argv, (long) envp);
}

/** Terminate process with exit code (does not return) */
void exit(int code){
    syscall1(SYS_exit, code);
    __builtin_unreachable();
}