#pragma once

#include "sys/types.h"
#include "stddef.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Standard file descriptors */
#define STDIN_FILENO  0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

/* lseek whence values */
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

/* access() mode flags */
#define F_OK 0
#define R_OK 4
#define W_OK 2
#define X_OK 1

/* File I/O */
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
int close(int fd);
off_t lseek(int fd, off_t offset, int whence);
ssize_t pread(int fd, void *buf, size_t count, off_t offset);
ssize_t pwrite(int fd, const void *buf, size_t count, off_t offset);

/* File descriptor manipulation */
int dup(int oldfd);
int dup2(int oldfd, int newfd);
int dup3(int oldfd, int newfd, int flags);
int pipe(int pipefd[2]);
int pipe2(int pipefd[2], int flags);

/* Process control */
pid_t fork(void);
int execve(const char *pathname, char *const argv[], char *const envp[]);
int execv(const char *pathname, char *const argv[]);
int execvp(const char *file, char *const argv[]);
int execvpe(const char *file, char *const argv[], char *const envp[]);
void _exit(int status);
pid_t getpid(void);
pid_t getppid(void);
pid_t getpgrp(void);
pid_t setsid(void);

/* User/Group IDs */
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
gid_t getegid(void);
int setuid(uid_t uid);
int setgid(gid_t gid);

/* Wait */
pid_t waitpid(pid_t pid, int *status, int options);
pid_t wait(int *status);

/* Signals */
int pause(void);
unsigned int alarm(unsigned int seconds);
unsigned int sleep(unsigned int seconds);

/* File system */
int access(const char *pathname, int mode);
int faccessat(int dirfd, const char *pathname, int mode, int flags);
int chdir(const char *path);
int fchdir(int fd);
char *getcwd(char *buf, size_t size);
int unlink(const char *pathname);
int unlinkat(int dirfd, const char *pathname, int flags);
int rmdir(const char *pathname);
int link(const char *oldpath, const char *newpath);
int symlink(const char *target, const char *linkpath);
ssize_t readlink(const char *pathname, char *buf, size_t bufsiz);
int chown(const char *pathname, uid_t owner, gid_t group);
int fchown(int fd, uid_t owner, gid_t group);
int fchownat(int dirfd, const char *pathname, uid_t owner, gid_t group, int flags);

/* Miscellaneous */
int isatty(int fd);
char *ttyname(int fd);
long sysconf(int name);

/* Compatibility - exit is in stdlib but commonly expected here too */
void exit(int status);

#ifdef __cplusplus
}
#endif