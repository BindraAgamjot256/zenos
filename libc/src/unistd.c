#include <unistd.h>
#include <sys/syscall.h>

extern long syscall3(long n, long a1, long a2, long a3);
extern long syscall1(long n, long a1);

ssize_t read(int fd, void *buf, size_t count) {
    return (ssize_t)syscall3(SYS_read, fd, (long)buf, (long)count);
}

ssize_t write(int fd, const void *buf, size_t count) {
    return (ssize_t)syscall3(SYS_write, fd, (long)buf, (long)count);
}

int close(int fd) {
    return (int)syscall1(SYS_close, fd);
}

off_t lseek(int fd, off_t offset, int whence) {
    return (off_t)syscall3(SYS_lseek, fd, offset, whence);
}
