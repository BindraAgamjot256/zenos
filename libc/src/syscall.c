/**
 * syscall.c - Raw system call interface for Zenos
 *
 * Provides low-level wrappers for invoking kernel syscalls using the
 * x86_64 syscall instruction. Arguments are passed via registers following
 * the System V AMD64 ABI syscall convention:
 *   - rax: syscall number
 *   - rdi, rsi, rdx, r10, r8, r9: arguments 1-6
 *   - rax: return value
 *
 * The syscall instruction clobbers rcx (saved RIP) and r11 (saved RFLAGS).
 */

#include "sys/syscall.h"

/**
 * Invoke a syscall with up to 6 arguments.
 *
 * Uses inline assembly to execute the syscall instruction with proper
 * register allocation per the AMD64 syscall ABI.
 */
long syscall(long syscall_number, long arg1, long arg2, long arg3, long arg4, long arg5, long arg6) {
    long ret;

    register long r10 __asm__("r10") = arg4;
    register long r8  __asm__("r8")  = arg5;
    register long r9  __asm__("r9")  = arg6;

    __asm__ __volatile__(
        "syscall"
        : "=a"(ret)
        : "a"(syscall_number), "D"(arg1), "S"(arg2), "d"(arg3),
          "r"(r10), "r"(r8), "r"(r9)
        : "rcx", "r11", "memory"
    );

    return ret;
}

/** Invoke syscall with no arguments */
long syscall0(const long n) {
    return syscall(n, 0, 0, 0, 0, 0, 0);
}

/** Invoke syscall with 1 argument */
long syscall1(const long n, const long a1) {
    return syscall(n, a1, 0, 0, 0, 0, 0);
}

/** Invoke syscall with 2 arguments */
long syscall2(const long n, const long a1, const long a2) {
    return syscall(n, a1, a2, 0, 0, 0, 0);
}

/** Invoke syscall with 3 arguments */
long syscall3(const long n, const long a1, const long a2, const long a3) {
    return syscall(n, a1, a2, a3, 0, 0, 0);
}

/** Invoke syscall with 4 arguments */
long syscall4(const long n, const long a1, const long a2, const long a3, const long a4) {
    return syscall(n, a1, a2, a3, a4, 0, 0);
}

/** Invoke syscall with 5 arguments */
long syscall5(const long n, const long a1, const long a2, const long a3, const long a4, const long a5) {
    return syscall(n, a1, a2, a3, a4, a5, 0);
}

/** Invoke syscall with 6 arguments */
long syscall6(const long n, const long a1, const long a2, const long a3, const long a4, const long a5, const long a6) {
    return syscall(n, a1, a2, a3, a4, a5, a6);
}