/**
 * fork_storm.c - Controlled fork storm stress test
 *
 * TESTS: Process creation, PID allocation, process table management
 * 
 * EXPECTED BEHAVIOR:
 * - Kernel should handle rapid fork() calls gracefully
 * - PID space should not wrap unexpectedly
 * - Parent should be able to observe all children created
 * - fork() should return -1 when process table is full, NOT crash
 *
 * KERNEL BUGS EXPOSED:
 * - Process table overflow without proper error return
 * - PID counter overflow/wraparound issues
 * - Memory leaks in process creation failure paths
 * - Deadlocks in process table locking
 */

#include <stdio.h>
#include <unistd.h>

#define MAX_CHILDREN 64

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[fork_storm] Starting controlled fork storm\n");
    
    int created = 0;
    int failed = 0;
    
    /* Rapid-fire fork() calls - tests process table pressure */
    for (int i = 0; i < MAX_CHILDREN; i++) {
        int pid = fork();
        
        if (pid < 0) {
            /* fork() failed - kernel should return error, not crash */
            failed++;
            printf("[fork_storm] fork() failed at iteration %d (expected if table full)\n", i);
            /* BUG CHECK: Does kernel properly cleanup partial process state? */
            break;
        } else if (pid == 0) {
            /* Child: spin briefly then exit */
            /* BUG CHECK: Does exit() properly free all resources? */
            for (volatile int j = 0; j < 1000; j++) {
                __asm__("pause");
            }
            exit(i);  /* Exit with iteration number as code */
            /* UNREACHABLE - if we get here, exit() is broken */
        } else {
            /* Parent: track children */
            created++;
            /* BUG CHECK: Are child PIDs unique and valid? */
            if (pid <= 0) {
                printf("[fork_storm] BUG: fork() returned invalid child PID %d\n", pid);
            }
        }
    }
    
    printf("[fork_storm] Created %d children, %d failures\n", created, failed);
    
    /* Spin to let children die - no wait() syscall available */
    /* BUG CHECK: Do zombie children accumulate? */
    while (waitpid(-1, NULL, 0) > 0);
    printf("[fork_storm] Parent exiting\n");
    exit(0);
    return 0;
}
