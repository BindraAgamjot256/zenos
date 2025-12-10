#include "sys/syscall.h"

long syscall0(const long n) {
    const long ret = syscall(n, 0, 0, 0, 0, 0, 0);
    return ret;
}

long syscall1(const long n, const long a1) {
    const long ret = syscall(n, a1, 0, 0, 0, 0, 0);
    return ret;
}

long syscall2(const long n, const long a1, const long a2) {
    const long ret = syscall(n, a1, a2, 0, 0, 0, 0);
    return ret;
}

long syscall3(const long n, const long a1, const long a2, const long a3) {
    const long ret = syscall(n, a1, a2, a3, 0, 0, 0);
    return ret;
}

long syscall(long syscall_number, long arg1, long arg2, long arg3, long arg4, long arg5, long arg6) {
    long ret;

    register long r10 __asm__("r10")
    =
    arg4;
    register long r8  __asm__("r8")
    =
    arg5;
    register long r9  __asm__("r9")
    =
    arg6;

    __asm__ __volatile__(
        "syscall"
    :
    "=a"(ret)
    :
    "a"(syscall_number), "D"(arg1), "S"(arg2), "d"(arg3),
            "r"(r10), "r"(r8), "r"(r9)
    :
    "rcx", "r11", "memory"
    )
    ;

    return ret;
}