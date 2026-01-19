/**
 * mem_exhaust.c - Memory exhaustion and fragmentation test
 *
 * TESTS: Stack allocation, memory limits, OOM handling
 *
 * EXPECTED BEHAVIOR:
 * - Process should be killed or receive error when exceeding limits
 * - Kernel should NOT crash when memory is exhausted
 * - Other processes should remain functional
 *
 * KERNEL BUGS EXPOSED:
 * - Kernel panic on OOM instead of graceful handling
 * - Stack overflow without guard page detection
 * - Memory corruption when allocation fails silently
 * - Lack of per-process memory limits
 *
 * NOTE: No malloc/brk available, so we stress stack allocation
 */

#include <stdio.h>
#include <unistd.h>
#include <string.h>

/* Recursive function to exhaust stack */
volatile int depth = 0;

void stack_dive(int level) {
    /* Allocate significant stack space */
    char buffer[4096];
    
    /* Touch the memory to ensure it's actually allocated */
    /* BUG CHECK: Is there a guard page? Will we get a fault? */
    memset(buffer, level & 0xFF, sizeof(buffer));
    
    depth = level;
    
    /* Print progress occasionally */
    if (level % 100 == 0) {
        printf("[mem_exhaust] Stack depth: %d\n", level);
    }
    
    /* Keep going until we crash or hit some limit */
    /* BUG CHECK: Does kernel handle stack overflow gracefully? */
    if (level < 10000) {
        stack_dive(level + 1);
    }
    
    /* If we return, check buffer wasn't corrupted */
    /* BUG CHECK: Memory corruption detection */
    if (buffer[0] != (char)(level & 0xFF)) {
        printf("[mem_exhaust] BUG: Stack corruption detected at level %d\n", level);
    }
}

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[mem_exhaust] Starting memory exhaustion test\n");
    
    /* Test 1: Stack exhaustion in child (protects parent) */
    int pid = fork();
    if (pid < 0) {
        printf("[mem_exhaust] Fork failed\n");
        exit(1);
    }
    if (pid == 0) {
        /* Child does the dangerous stack dive */
        printf("[mem_exhaust] Child starting stack dive\n");
        stack_dive(1);
        printf("[mem_exhaust] Child survived! Max depth: %d\n", depth);
        exit(0);
    }
    
    /* Parent waits and observes */
    printf("[mem_exhaust] Parent waiting for child\n");
    for (volatile int i = 0; i < 200000; i++) {
        __asm__("pause");
    }
    
    /* Test 2: Large stack allocations */
    printf("[mem_exhaust] Testing large stack allocation\n");
    {
        /* Try to allocate a large buffer on stack */
        /* BUG CHECK: Does this cause stack overflow? */
        char big_buffer[65536];
        memset(big_buffer, 0xAA, sizeof(big_buffer));
        
        /* Verify it worked */
        int ok = 1;
        for (int i = 0; i < 100; i++) {
            if (big_buffer[i * 650] != (char)0xAA) {
                ok = 0;
                break;
            }
        }
        printf("[mem_exhaust] Large buffer test: %s\n", ok ? "PASS" : "FAIL");
    }
    
    printf("[mem_exhaust] Test complete\n");
    exit(0);
    return 0;
}
