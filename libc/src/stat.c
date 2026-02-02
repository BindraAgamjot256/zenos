/**
 * stat.c - File status operations
 *
 * Provides stat(), fstat(), lstat() for getting file information.
 */

#include "sys/stat.h"
#include "sys/syscall.h"
#include "errno.h"

/* Helper to set errno and return -1 on syscall error */
static inline long syscall_ret(long ret) {
    if (ret < 0 && ret > -4096) {
        errno = (int)(-ret);
        return -1;
    }
    return ret;
}

int stat(const char *pathname, struct stat *statbuf) {
    return (int)syscall_ret(syscall2(SYS_stat, (long)pathname, (long)statbuf));
}

int fstat(int fd, struct stat *statbuf) {
    return (int)syscall_ret(syscall2(SYS_fstat, fd, (long)statbuf));
}

int lstat(const char *pathname, struct stat *statbuf) {
    return (int)syscall_ret(syscall2(SYS_lstat, (long)pathname, (long)statbuf));
}
