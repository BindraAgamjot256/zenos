#include <sys/syscall.h>

long syscall0(long n) {
    long ret;
    __asm__ __volatile__("syscall" : "=a"(ret) : "a"(n) : "rcx", "r11", "memory");
    return ret;
}

long syscall1(long n, long a1) {
    long ret;
    __asm__ __volatile__("syscall" : "=a"(ret) : "a"(n), "D"(a1) : "rcx", "r11", "memory");
    return ret;
}

long syscall2(long n, long a1, long a2) {
    long ret;
    __asm__ __volatile__("syscall" : "=a"(ret) : "a"(n), "D"(a1), "S"(a2) : "rcx", "r11", "memory");
    return ret;
}

long syscall3(long n, long a1, long a2, long a3) {
    long ret;
    __asm__ __volatile__("syscall" : "=a"(ret) : "a"(n), "D"(a1), "S"(a2), "d"(a3) : "rcx", "r11", "memory");
    return ret;
}
