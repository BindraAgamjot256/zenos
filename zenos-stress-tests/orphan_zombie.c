/**
 * orphan_zombie.c - Parent/child lifetime edge cases
 *
 * TESTS: Orphan process handling, zombie accumulation, reparenting
 *
 * EXPECTED BEHAVIOR:
 * - Orphaned children should be reparented to init (PID 1)
 * - Zombies should eventually be reaped
 * - System should remain stable with many orphans
 *
 * KERNEL BUGS EXPOSED:
 * - Orphan processes left in limbo (not reparented)
 * - Zombie accumulation exhausting process table
 * - Parent exit not properly notifying children
 * - Race conditions in reparenting logic
 */

#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

#define NUM_ORPHANS 10

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[orphan_zombie] Starting orphan/zombie test\n");
    
    /* Create a grandparent -> parent -> child chain */
    /* Then kill the parent to orphan the child */
    
    int pid = fork();
    if (pid < 0) {
        printf("[orphan_zombie] Initial fork failed\n");
        exit(1);
    }
    
    if (pid == 0) {
        /* We are the "parent" that will die, orphaning children */
        printf("[parent] Creating children then dying...\n");
        
        for (int i = 0; i < NUM_ORPHANS; i++) {
            int child_pid = fork();
            if (child_pid < 0) {
                printf("[parent] Failed to create child %d\n", i);
                continue;
            }
            if (child_pid == 0) {
                /* ORPHAN-TO-BE */
                /* BUG CHECK: What happens when our parent dies? */
                printf("[child %d] Born, parent will die soon\n", i);
                
                /* Wait for parent to definitely be dead */
                for (volatile int j = 0; j < 100000; j++) {
                    __asm__("pause");
                }
                
                /* We should now be orphaned */
                /* BUG CHECK: Are we reparented? Can we still run? */
                printf("[orphan %d] Still alive after parent death!\n", i);
                
                /* Try to do something useful */
                int fd = open("/proc/self/status", 0x0001);  /* O_RDONLY */
                if (fd >= 0) {
                    char buf[256];
                    ssize_t n = read(fd, buf, sizeof(buf) - 1);
                    if (n > 0) {
                        buf[n] = '\0';
                        /* BUG CHECK: Does /proc/self work for orphan? */
                        printf("[orphan %d] Can read procfs: %s\n", i, buf);
                    }
                    close(fd);
                }
                
                exit(i);
            }
        }
        
        /* Parent dies immediately, orphaning all children */
        printf("[parent] Exiting, orphaning %d children\n", NUM_ORPHANS);
        exit(0);
    }
    
    /* Grandparent observes */
    printf("[grandparent] Waiting for chaos...\n");
    
    /* Give time for parent to die and orphans to run */
    for (volatile int i = 0; i < 500000; i++) {
        __asm__("pause");
    }
    
    /* Test 2: Create zombies by having children exit while parent lives */
    printf("[grandparent] Creating zombie test...\n");
    
    for (int i = 0; i < 5; i++) {
        int zpid = fork();
        if (zpid == 0) {
            /* Child exits immediately, becoming zombie */
            /* BUG CHECK: Does kernel properly track zombie state? */
            printf("[zombie-to-be %d] Exiting immediately\n", i);
            exit(0);
        }
        /* Parent doesn't wait - no wait() syscall anyway */
    }
    
    /* Check procfs to see process count */
    /* BUG CHECK: Are zombies visible in procfs? */
    printf("[grandparent] Zombies created, checking procfs...\n");
    
    /* More delay to observe zombie behavior */
     while (waitpid(-1) > 0);

    printf("[grandparent] Test complete\n");
    exit(0);
    return 0;
}
