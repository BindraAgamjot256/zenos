#pragma once
#include "stdint.h"
#include "sys/types.h"

/* The bitmask type */
typedef uint64_t FileOpenOptions;

/* Access modes (mutually exclusive, lowest bits just like O_* flags) */
#define O_RDONLY    0
#define O_WRONLY    1
#define O_RDWR      2
#define O_ACCMODE   3

/* File creation and status flags */
#define O_CREAT     0100
#define O_EXCL      0200
#define O_NOCTTY    0400
#define O_TRUNC     01000
#define O_APPEND    02000
#define O_NONBLOCK  04000
#define O_SYNC      010000
#define O_CLOEXEC   02000000
#define O_DIRECTORY 0200000
#define O_NOFOLLOW  0400000

/* Legacy aliases for backward compatibility */
#define FILE_READ_ONLY   ((FileOpenOptions)O_RDONLY)
#define FILE_WRITE_ONLY  ((FileOpenOptions)O_WRONLY)
#define FILE_READ_WRITE  ((FileOpenOptions)O_RDWR)
#define FILE_CREATE        ((FileOpenOptions)O_CREAT)
#define FILE_EXCLUSIVE     ((FileOpenOptions)O_EXCL)
#define FILE_NOCTTY        ((FileOpenOptions)O_NOCTTY)
#define FILE_TRUNCATE      ((FileOpenOptions)O_TRUNC)
#define FILE_APPEND        ((FileOpenOptions)O_APPEND)
#define FILE_NONBLOCK      ((FileOpenOptions)O_NONBLOCK)
#define FILE_SYNC          ((FileOpenOptions)O_SYNC)
#define FILE_CLOSE_ON_EXEC ((FileOpenOptions)O_CLOEXEC)

/* Helper macros */
#define FILE_OPTIONS_HAS(opts, flag) (((opts) & (flag)) != 0)
#define FILE_OPTIONS_ADD(opts, flag) ((opts) |= (flag))
#define FILE_OPTIONS_REMOVE(opts, flag) ((opts) &= ~(flag))

/* fcntl commands */
#define F_DUPFD     0
#define F_GETFD     1
#define F_SETFD     2
#define F_GETFL     3
#define F_SETFL     4
#define F_DUPFD_CLOEXEC 1030

/* fcntl fd flags */
#define FD_CLOEXEC  1

/* openat special fd value */
#define AT_FDCWD    (-100)

/* unlinkat flags */
#define AT_REMOVEDIR 0x200

/* faccessat flags */
#define AT_EACCESS          0x200
#define AT_SYMLINK_NOFOLLOW 0x100

#ifdef __cplusplus
extern "C" {
#endif

int open(const char *pathname, int flags, ...);
int openat(int dirfd, const char *pathname, int flags, ...);
int creat(const char *pathname, mode_t mode);
int fcntl(int fd, int cmd, ...);

/* Directory operations */
int mkdir(const char *pathname, mode_t mode);
int mkdirat(int dirfd, const char *pathname, mode_t mode);
int rename(const char *oldpath, const char *newpath);
int renameat(int olddirfd, const char *oldpath, int newdirfd, const char *newpath);

/* File mode */
int chmod(const char *pathname, mode_t mode);
int fchmod(int fd, mode_t mode);

#ifdef __cplusplus
}
#endif