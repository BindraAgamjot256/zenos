#pragma once
#define SYS_read    0
#define SYS_write   1
#define SYS_open    2
#define SYS_close   3
#define SYS_lseek   8
#define SYS_fork    57
#define SYS_pause   34

long syscall(long syscall_number, long arg1, long arg2, long arg3, long arg4, long arg5, long arg6);

long syscall0(long n);

long syscall1(long n, long a1);

long syscall2(long n, long a1, long a2);

long syscall3(long n, long a1, long a2, long a3);