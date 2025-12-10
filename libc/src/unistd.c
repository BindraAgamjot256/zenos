#include <unistd.h>
#include <sys/syscall.h>


ssize_t read(const int fd, void *buf, const size_t count) {
    return syscall3(SYS_read, fd, (long) buf, (long) count);
}

ssize_t write(const int fd, const void *buf, const size_t count) {
    return syscall3(SYS_write, fd, (long) buf, (long) count);
}

int close(int fd) {
    return (int) syscall1(SYS_close, fd);
}

off_t lseek(const int fd, const off_t offset, const int whence) {
    return syscall3(SYS_lseek, fd, offset, whence);
}