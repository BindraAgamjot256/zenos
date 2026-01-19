/**
 * syscall_abuse.c - Syscall edge cases and invalid arguments
 *
 * TESTS: Syscall validation, error handling, NULL pointer handling
 *
 * EXPECTED BEHAVIOR:
 * - Invalid arguments should return appropriate error codes
 * - NULL pointers should be rejected, not crash the kernel
 * - Invalid file descriptors should return errors
 * - Kernel should NEVER crash from userspace input
 *
 * KERNEL BUGS EXPOSED:
 * - Kernel panics from bad userspace pointers
 * - Missing argument validation
 * - Integer overflow in size parameters
 * - Use of uninitialized kernel memory
 * - Buffer overflows from unchecked sizes
 */

#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/syscall.h>

/* We'll call syscalls directly to pass truly bad arguments */

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[syscall_abuse] Starting syscall abuse test\n");
    printf("[syscall_abuse] If you see this, kernel survived so far!\n");
    
    long ret;
    
    /* Test 1: read() with NULL buffer */
    printf("[syscall_abuse] Testing read(0, NULL, 100)...\n");
    ret = syscall3(SYS_read, 0, 0, 100);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 2: write() with NULL buffer */
    printf("[syscall_abuse] Testing write(1, NULL, 100)...\n");
    ret = syscall3(SYS_write, 1, 0, 100);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 3: read() with invalid pointer (kernel address) */
    printf("[syscall_abuse] Testing read with kernel-space pointer...\n");
    ret = syscall3(SYS_read, 0, 0xFFFFFFFF80000000UL, 100);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 4: write() with kernel-space pointer */
    printf("[syscall_abuse] Testing write with kernel-space pointer...\n");
    ret = syscall3(SYS_write, 1, 0xFFFFFFFF80000000UL, 100);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 5: Invalid file descriptor */
    printf("[syscall_abuse] Testing read on invalid fd (9999)...\n");
    char buf[16];
    ret = syscall3(SYS_read, 9999, (long)buf, 16);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 6: Negative file descriptor */
    printf("[syscall_abuse] Testing read on negative fd (-1)...\n");
    ret = syscall3(SYS_read, -1, (long)buf, 16);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 7: Huge size parameter (potential overflow) */
    printf("[syscall_abuse] Testing read with huge size...\n");
    ret = syscall3(SYS_read, 0, (long)buf, 0x7FFFFFFFFFFFFFFFULL);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 8: open() with NULL path */
    printf("[syscall_abuse] Testing open(NULL, 0)...\n");
    ret = syscall2(SYS_open, 0, 0);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 9: open() with kernel-space path */
    printf("[syscall_abuse] Testing open with kernel pointer...\n");
    ret = syscall2(SYS_open, 0xFFFFFFFF80000000UL, 0);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 10: close() on invalid fd */
    printf("[syscall_abuse] Testing close(9999)...\n");
    ret = syscall1(SYS_close, 9999);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 11: lseek() on invalid fd */
    printf("[syscall_abuse] Testing lseek on invalid fd...\n");
    ret = syscall3(SYS_lseek, 9999, 0, 0);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 12: lseek() with invalid whence */
    printf("[syscall_abuse] Testing lseek with bad whence...\n");
    int fd = open("/proc/version", O_RDONLY);
    if (fd >= 0) {
        ret = syscall3(SYS_lseek, fd, 0, 999);  /* Invalid whence */
        printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
        close(fd);
    }
    
    /* Test 13: execve() with NULL arguments */
    printf("[syscall_abuse] Testing execve(NULL, NULL, NULL)...\n");
    ret = syscall3(SYS_execve, 0, 0, 0);
    printf("[syscall_abuse] Result: %ld (expected: error, or we wouldn't be here)\n", ret);
    
    /* Test 14: execve() with invalid path */
    printf("[syscall_abuse] Testing execve with kernel pointer...\n");
    ret = syscall3(SYS_execve, 0xFFFFFFFF80000000UL, 0, 0);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 15: execve() with non-existent file */
    printf("[syscall_abuse] Testing execve with non-existent file...\n");
    const char *nofile = "/nonexistent/path/to/binary";
    ret = syscall3(SYS_execve, (long)nofile, 0, 0);
    printf("[syscall_abuse] Result: %ld (expected: error)\n", ret);
    
    /* Test 16: Double close */
    printf("[syscall_abuse] Testing double close...\n");
    fd = open("/proc/version", O_RDONLY);
    if (fd >= 0) {
        close(fd);
        ret = syscall1(SYS_close, fd);  /* Close again */
        printf("[syscall_abuse] Double close result: %ld (expected: error)\n", ret);
    }
    
    /* Test 17: Read/write on closed fd */
    printf("[syscall_abuse] Testing read on closed fd...\n");
    fd = open("/proc/version", O_RDONLY);
    if (fd >= 0) {
        close(fd);
        ret = syscall3(SYS_read, fd, (long)buf, 16);
        printf("[syscall_abuse] Read on closed fd: %ld (expected: error)\n", ret);
    }
    
    /* Test 18: exit() with various codes (last one kills us) */
    printf("[syscall_abuse] Testing fork+exit with weird codes...\n");
    int pid = fork();
    if (pid == 0) {
        /* Child: exit with max value */
        exit(-1);  /* Or 255, or whatever fits */
    }
    
    /* Small delay */
    for (volatile int i = 0; i < 10000; i++) {
        __asm__("pause");
    }
    
    printf("[syscall_abuse] All tests survived! Kernel is robust.\n");
    exit(0);
    return 0;
}
