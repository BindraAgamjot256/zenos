#pragma once
#define SYS_read    0
#define SYS_write   1
#define SYS_open    2
#define SYS_close   3
#define SYS_stat    4
#define SYS_fstat   5
#define SYS_lstat   6
#define SYS_poll    7
#define SYS_lseek   8
#define SYS_mmap    9
#define SYS_mprotect 10
#define SYS_munmap  11
#define SYS_brk     12
#define SYS_ioctl   16
#define SYS_access  21
#define SYS_pipe    22
#define SYS_dup     32
#define SYS_dup2    33
#define SYS_pause   34
#define SYS_getpid  39
#define SYS_fork    57
#define SYS_execve  59
#define SYS_exit    60
#define SYS_waitpid 61
#define SYS_kill    62
#define SYS_uname   63
#define SYS_fcntl   72
#define SYS_getdents 217 //todo: getdents is its own syscall, not identical to getdents64
#define SYS_getcwd  79
#define SYS_chdir   80
#define SYS_mkdir   83
#define SYS_rmdir   84
#define SYS_unlink  87
#define SYS_readlink 89
#define SYS_chmod   90
#define SYS_chown   92
#define SYS_getuid  102
#define SYS_getgid  104
#define SYS_geteuid 107
#define SYS_getegid 108
#define SYS_getppid 110
#define SYS_getpgrp 111
#define SYS_setsid  112
#define SYS_setuid  105
#define SYS_setgid  106
#define SYS_openat  257
#define SYS_mkdirat 258
#define SYS_fchownat 260
#define SYS_unlinkat 263
#define SYS_renameat 264
#define SYS_faccessat 269
#define SYS_getdents64 217
#define SYS_dup3    292
#define SYS_pipe2   293

long syscall(long syscall_number, long arg1, long arg2, long arg3, long arg4, long arg5, long arg6);

long syscall0(long n);

long syscall1(long n, long a1);

long syscall2(long n, long a1, long a2);

long syscall3(long n, long a1, long a2, long a3);

long syscall4(long n, long a1, long a2, long a3, long a4);

long syscall5(long n, long a1, long a2, long a3, long a4, long a5);

long syscall6(long n, long a1, long a2, long a3, long a4, long a5, long a6);