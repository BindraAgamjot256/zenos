/**
 * rapid_spawn.c - Rapid process creation and teardown
 *
 * TESTS: Process lifecycle, resource cleanup, scheduler queue management
 *
 * EXPECTED BEHAVIOR:
 * - Each fork+immediate exit should fully release resources
 * - Process count should remain stable (no zombie accumulation)
 * - Scheduler should handle rapidly appearing/disappearing processes
 *
 * KERNEL BUGS EXPOSED:
 * - Memory leaks in process teardown
 * - Zombie process accumulation (no reaper)
 * - Scheduler corruption when processes exit during scheduling
 * - Race between exit() and scheduler picking process to run
 */

#include <stdio.h>
#include <unistd.h>

#define ITERATIONS 100

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[rapid_spawn] Starting rapid spawn/teardown test\n");
    
    for (int i = 0; i < ITERATIONS; i++) {
        int pid = fork();
        
        if (pid < 0) {
            /* BUG CHECK: Did previous rapid exits leave kernel in bad state? */
            printf("[rapid_spawn] fork() failed at iteration %d\n", i);
            /* Try to recover - maybe resources will free up */
            for (volatile int j = 0; j < 10000; j++) {
                __asm__("pause");
            }
            continue;
        } else if (pid == 0) {
            /* Child: Exit IMMEDIATELY */
            /* BUG CHECK: Does exit() during fork() return path cause issues? */
            /* BUG CHECK: Is process state consistent when exit() is called this fast? */
            exit(0);
            /* UNREACHABLE */
        }
        
        /* Parent: No delay, immediately fork again */
        /* BUG CHECK: Does kernel handle overlapping fork/exit correctly? */
        
        /* Occasional status print */
        if (i % 20 == 0) {
            printf("[rapid_spawn] Completed %d iterations\n", i);
        }
    }
    
    printf("[rapid_spawn] Test complete, spawned %d short-lived processes\n", ITERATIONS);
    
    /* Give kernel time to reap zombies (if it does) */
    while (waitpid(-1, NULL, 0) > 0);
    exit(0);
    return 0;
}
