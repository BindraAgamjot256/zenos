/**
 * fcntl.c - File control operations
 *
 * Provides open(), openat(), fcntl() and related file operations.
 */

#include "fcntl.h"
#include "sys/syscall.h"
#include "stdarg.h"
#include "errno.h"

/* Helper to set errno and return -1 on syscall error */
static inline long syscall_ret(long ret) {
    if (ret < 0 && ret > -4096) {
        errno = (int)(-ret);
        return -1;
    }
    return ret;
}

/**
 * Open a file.
 * @param pathname  Path to the file
 * @param flags     O_RDONLY, O_WRONLY, O_CREAT, O_TRUNC (can be OR'd)
 * @param ...       mode_t mode (required if O_CREAT is set)
 * @return          File descriptor on success, -1 on error
 */
int open(const char *pathname, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = va_arg(ap, mode_t);
        va_end(ap);
    }
    (void)mode; /* Zenos doesn't use mode yet */
    return (int)syscall_ret(syscall2(SYS_open, (long)pathname, (long)flags));
}

/**
 * Open a file relative to a directory file descriptor.
 * @param dirfd     Directory file descriptor (or AT_FDCWD)
 * @param pathname  Path to the file
 * @param flags     Open flags
 * @param ...       mode_t mode (required if O_CREAT is set)
 * @return          File descriptor on success, -1 on error
 */
int openat(int dirfd, const char *pathname, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = va_arg(ap, mode_t);
        va_end(ap);
    }
    return (int)syscall_ret(syscall4(SYS_openat, dirfd, (long)pathname, flags, mode));
}

/**
 * Create a file (equivalent to open with O_CREAT|O_WRONLY|O_TRUNC).
 */
int creat(const char *pathname, mode_t mode) {
    return open(pathname, O_CREAT | O_WRONLY | O_TRUNC, mode);
}

/**
 * File control operations.
 */
int fcntl(int fd, int cmd, ...) {
    va_list ap;
    long arg = 0;

    va_start(ap, cmd);
    switch (cmd) {
        case F_DUPFD:
        case F_DUPFD_CLOEXEC:
        case F_SETFD:
        case F_SETFL:
            arg = va_arg(ap, long);
            break;
        default:
            break;
    }
    va_end(ap);

    return (int)syscall_ret(syscall3(SYS_fcntl, fd, cmd, arg));
}

/**
 * Create a directory.
 */
int mkdir(const char *pathname, mode_t mode) {
    return (int)syscall_ret(syscall2(SYS_mkdir, (long)pathname, mode));
}

/**
 * Create a directory relative to a directory file descriptor.
 */
int mkdirat(int dirfd, const char *pathname, mode_t mode) {
    return (int)syscall_ret(syscall3(SYS_mkdirat, dirfd, (long)pathname, mode));
}

/**
 * Rename a file.
 */
int rename(const char *oldpath, const char *newpath) {
    return renameat(AT_FDCWD, oldpath, AT_FDCWD, newpath);
}

/**
 * Rename a file relative to directory file descriptors.
 */
int renameat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath) {
    return (int)syscall_ret(syscall4(SYS_renameat, olddirfd, (long)oldpath, newdirfd, (long)newpath));
}

/**
 * Change file mode.
 */
int chmod(const char *pathname, mode_t mode) {
    return (int)syscall_ret(syscall2(SYS_chmod, (long)pathname, mode));
}

/**
 * Change file mode by file descriptor.
 */
int fchmod(int fd, mode_t mode) {
    (void)fd;
    (void)mode;
    errno = ENOSYS;
    return -1;
}
