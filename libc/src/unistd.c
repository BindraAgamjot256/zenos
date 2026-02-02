/**
 * unistd.c - POSIX-like system call wrappers
 *
 * Provides familiar POSIX I/O and process functions by wrapping
 * the underlying Zenos syscalls where present, and saying fuck you and lying where not.
 */

#include "unistd.h"
#include "sys/syscall.h"
#include "stdlib.h"
#include "string.h"
#include "errno.h"

/* Helper to set errno and return -1 on syscall error */
static inline long syscall_ret(long ret) {
    if (ret < 0 && ret > -4096) {
        errno = (int)(-ret);
        return -1;
    }
    return ret;
}

/* ========== File I/O ========== */

ssize_t read(const int fd, void *buf, const size_t count) {
    return syscall_ret(syscall3(SYS_read, fd, (long)buf, (long)count));
}

ssize_t write(const int fd, const void *buf, const size_t count) {
    return syscall_ret(syscall3(SYS_write, fd, (long)buf, (long)count));
}

int close(int fd) {
    return (int)syscall_ret(syscall1(SYS_close, fd));
}

off_t lseek(const int fd, const off_t offset, const int whence) {
    return syscall_ret(syscall3(SYS_lseek, fd, offset, whence));
}

ssize_t pread(int fd, void *buf, size_t count, off_t offset) {
    off_t saved = lseek(fd, 0, SEEK_CUR);
    if (saved < 0) return saved;
    if (lseek(fd, offset, SEEK_SET) < 0) return -1;
    ssize_t ret = read(fd, buf, count);
    lseek(fd, saved, SEEK_SET);
    return ret;
}

ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset) {
    off_t saved = lseek(fd, 0, SEEK_CUR);
    if (saved < 0) return saved;
    if (lseek(fd, offset, SEEK_SET) < 0) return -1;
    ssize_t ret = write(fd, buf, count);
    lseek(fd, saved, SEEK_SET);
    return ret;
}

/* ========== File descriptor manipulation ========== */

int dup(int oldfd) {
    return (int)syscall_ret(syscall1(SYS_dup, oldfd));
}

int dup2(int oldfd, int newfd) {
    return (int)syscall_ret(syscall2(SYS_dup2, oldfd, newfd));
}

int dup3(int oldfd, int newfd, int flags) {
    return (int)syscall_ret(syscall3(SYS_dup3, oldfd, newfd, flags));
}

int pipe(int pipefd[2]) {
    return (int)syscall_ret(syscall1(SYS_pipe, (long)pipefd));
}

int pipe2(int pipefd[2], int flags) {
    return (int)syscall_ret(syscall2(SYS_pipe2, (long)pipefd, flags));
}

/* ========== Process control ========== */

pid_t fork(void) {
    return (pid_t)syscall_ret(syscall0(SYS_fork));
}

int execve(const char *pathname, char *const argv[], char *const envp[]) {
    return (int)syscall_ret(syscall3(SYS_execve, (long)pathname, (long)argv, (long)envp));
}

int execv(const char *pathname, char *const argv[]) {
    return execve(pathname, argv, environ);
}

int execvp(const char *file, char *const argv[]) {
    return execvpe(file, argv, environ);
}

int execvpe(const char *file, char *const argv[], char *const envp[]) {
    /* If file contains a slash, treat it as a path */
    if (strchr(file, '/') != NULL) {
        return execve(file, argv, envp);
    }

    /* Search PATH for the executable */
    const char *path = getenv("PATH");
    if (path == NULL) {
        path = "/bin:/usr/bin";
    }

    char buf[4096];
    const char *p = path;

    while (*p) {
        const char *end = p;
        while (*end && *end != ':') end++;

        size_t dir_len = (size_t)(end - p);
        size_t file_len = strlen(file);

        if (dir_len + 1 + file_len + 1 > sizeof(buf)) {
            p = (*end) ? end + 1 : end;
            continue;
        }

        memcpy(buf, p, dir_len);
        buf[dir_len] = '/';
        memcpy(buf + dir_len + 1, file, file_len);
        buf[dir_len + 1 + file_len] = '\0';

        execve(buf, argv, envp);
        if (errno != ENOENT) {
            return -1;
        }

        p = (*end) ? end + 1 : end;
    }

    errno = ENOENT;
    return -1;
}

void _exit(int status) {
    syscall1(SYS_exit, status);
    __builtin_unreachable();
}

void exit(int status) {
    _exit(status);
}

pid_t getpid(void) {
    return (pid_t)syscall0(SYS_getpid);
}

pid_t getppid(void) {
    return (pid_t)syscall0(SYS_getppid);
}

pid_t getpgrp(void) {
    return (pid_t)syscall0(SYS_getpgrp);
}

pid_t setsid(void) {
    return (pid_t)syscall_ret(syscall0(SYS_setsid));
}

/* ========== User/Group IDs ========== */

uid_t getuid(void) {
    return (uid_t)syscall0(SYS_getuid);
}

uid_t geteuid(void) {
    return (uid_t)syscall0(SYS_geteuid);
}

gid_t getgid(void) {
    return (gid_t)syscall0(SYS_getgid);
}

gid_t getegid(void) {
    return (gid_t)syscall0(SYS_getegid);
}

int setuid(uid_t uid) {
    return (int)syscall_ret(syscall1(SYS_setuid, uid));
}

int setgid(gid_t gid) {
    return (int)syscall_ret(syscall1(SYS_setgid, gid));
}

/* ========== Wait ========== */

pid_t waitpid(pid_t pid, int *status, int options) {
    (void)status;
    (void)options;
    return (pid_t)syscall_ret(syscall1(SYS_waitpid, pid));
}

pid_t wait(int *status) {
    return waitpid(-1, status, 0);
}

/* ========== Signals ========== */

int pause(void) {
    return (int)syscall_ret(syscall0(SYS_pause));
}

unsigned int alarm(unsigned int seconds) {
    (void)seconds;
    return 0; /* Not implemented */
}

unsigned int sleep(unsigned int seconds) {
    (void)seconds;
    pause();
    return 0;
}

/* ========== File system ========== */

int access(const char *pathname, int mode) {
    return (int)syscall_ret(syscall2(SYS_access, (long)pathname, mode));
}

int faccessat(int dirfd, const char *pathname, int mode, int flags) {
    return (int)syscall_ret(syscall4(SYS_faccessat, dirfd, (long)pathname, mode, flags));
}

int chdir(const char *path) {
    return (int)syscall_ret(syscall1(SYS_chdir, (long)path));
}

int fchdir(int fd) {
    (void)fd;
    errno = ENOSYS;
    return -1;
}

char *getcwd(char *buf, size_t size) {
    long ret = syscall2(SYS_getcwd, (long)buf, (long)size);
    if (ret < 0 && ret > -4096) {
        errno = (int)(-ret);
        return NULL;
    }
    return buf;
}

int unlink(const char *pathname) {
    return (int)syscall_ret(syscall1(SYS_unlink, (long)pathname));
}

int unlinkat(int dirfd, const char *pathname, int flags) {
    return (int)syscall_ret(syscall3(SYS_unlinkat, dirfd, (long)pathname, flags));
}

int rmdir(const char *pathname) {
    return (int)syscall_ret(syscall1(SYS_rmdir, (long)pathname));
}

int link(const char *oldpath, const char *newpath) {
    (void)oldpath;
    (void)newpath;
    errno = ENOSYS;
    return -1;
}

int symlink(const char *target, const char *linkpath) {
    (void)target;
    (void)linkpath;
    errno = ENOSYS;
    return -1;
}

ssize_t readlink(const char *pathname, char *buf, size_t bufsiz) {
    return syscall_ret(syscall3(SYS_readlink, (long)pathname, (long)buf, (long)bufsiz));
}

int chown(const char *pathname, uid_t owner, gid_t group) {
    return (int)syscall_ret(syscall3(SYS_chown, (long)pathname, owner, group));
}

int fchown(int fd, uid_t owner, gid_t group) {
    (void)fd;
    (void)owner;
    (void)group;
    errno = ENOSYS;
    return -1;
}

int fchownat(int dirfd, const char *pathname, uid_t owner, gid_t group, int flags) {
    return (int)syscall_ret(syscall5(SYS_fchownat, dirfd, (long)pathname, owner, group, flags));
}

/* ========== Miscellaneous ========== */

int isatty(int fd) {
    (void)fd;
    return 1; /* Assume everything is a tty for now */
}

char *ttyname(int fd) {
    (void)fd;
    return "/dev/tty";
}

long sysconf(int name) {
    (void)name;
    errno = ENOSYS;
    return -1;
}