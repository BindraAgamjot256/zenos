/**
 * sched_fairness.c - CPU hog vs sleeping process scheduler test
 *
 * TESTS: Scheduler fairness, pause() syscall, CPU-bound vs I/O-bound balance
 *
 * EXPECTED BEHAVIOR:
 * - Sleeping processes should wake up and get CPU time
 * - CPU hogs should not completely starve other processes
 * - pause() should actually suspend the process (if implemented)
 *
 * KERNEL BUGS EXPOSED:
 * - Scheduler starvation of low-priority or sleeping processes
 * - pause() not properly blocking (busy-wait instead)
 * - Incorrect time slice allocation
 * - Priority inversion issues
 */

#include <stdio.h>
#include <unistd.h>

#define NUM_HOGS 3
#define NUM_SLEEPERS 2
#define HOG_ITERATIONS 500000

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[sched_fairness] Starting scheduler fairness test\n");
    
    /* Spawn CPU hogs */
    for (int i = 0; i < NUM_HOGS; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[sched_fairness] Failed to create hog %d\n", i);
            continue;
        }
        if (pid == 0) {
            /* CPU HOG child */
            printf("[hog %d] Starting CPU-intensive work\n", i);
            
            volatile long counter = 0;
            for (int j = 0; j < HOG_ITERATIONS; j++) {
                /* Pure CPU burn - no yielding */
                counter += j * j;
                /* BUG CHECK: Does scheduler preempt this? */
                /* If not, other processes will starve */
            }
            
            printf("[hog %d] Finished after counting to %ld\n", i, counter);
            exit(0);
        }
    }
    
    /* Spawn sleepers that use pause() */
    for (int i = 0; i < NUM_SLEEPERS; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[sched_fairness] Failed to create sleeper %d\n", i);
            continue;
        }
        if (pid == 0) {
            /* SLEEPER child */
            printf("[sleeper %d] Going to sleep with pause()\n", i);
            
            /* BUG CHECK: Does pause() actually block or spin? */
            /* BUG CHECK: Will we ever wake up without signals? */
            int ret = pause();
            
            /* If pause() returns, something happened */
            printf("[sleeper %d] pause() returned %d\n", i, ret);
            
            /* Do some work to prove we got CPU time */
            for (volatile int j = 0; j < 1000; j++) {
                __asm__("pause");
            }
            
            printf("[sleeper %d] Exiting\n", i);
            exit(0);
        }
    }
    
    /* Parent monitors - if we can print, scheduler isn't totally broken */
    printf("[sched_fairness] Parent observing...\n");
    
    for (int i = 0; i < 5; i++) {
        /* Brief delay */
        for (volatile int j = 0; j < 100000; j++) {
        }
        /* BUG CHECK: Can parent get CPU time among all the hogs? */
        printf("[sched_fairness] Parent heartbeat %d\n", i);
    }
    
    printf("[sched_fairness] Test complete\n");
    // reap zombies.
    while (waitpid(-1, NULL, 0) > 0);
    exit(0);
    return 0;
}
