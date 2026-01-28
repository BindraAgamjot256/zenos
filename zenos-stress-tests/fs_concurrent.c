/**
 * fs_concurrent.c - Concurrent filesystem access from multiple processes
 *
 * TESTS: File descriptor isolation, concurrent file operations, VFS locking
 *
 * EXPECTED BEHAVIOR:
 * - Each process should have isolated file descriptors (after fork)
 * - Concurrent reads should not corrupt data
 * - Concurrent writes to same file should serialize properly
 * - File offsets should be per-fd, not per-file
 *
 * KERNEL BUGS EXPOSED:
 * - File descriptor table corruption
 * - Race conditions in VFS layer
 * - Incorrect file offset sharing after fork
 * - Data corruption from concurrent access
 * - Deadlocks in filesystem locks
 */

#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>

#define NUM_READERS 4
#define NUM_WRITERS 2
#define TEST_FILE "/tst-conc.txt"

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf("[fs_concurrent] Starting concurrent filesystem test\n");
    
    /* Create test file */
    int fd = open(TEST_FILE, FILE_CREATE | FILE_READ_WRITE);
    if (fd < 0) {
        printf("[fs_concurrent] Failed to create test file\n");
        printf("[fs_concurrent] error: %d", fd);
        exit(-1);
    }
    
    /* Write initial content */
    const char *initial = "INITIAL_CONTENT_0123456789ABCDEF";
    write(fd, initial, strlen(initial));
    close(fd);
    
    printf("[fs_concurrent] Test file created\n");
    
    /* Spawn concurrent readers */
    for (int i = 0; i < NUM_READERS; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[fs_concurrent] Failed to create reader %d\n", i);
            continue;
        }
        if (pid == 0) {
            /* READER process */
            char buf[64];
            
            for (int j = 0; j < 10; j++) {
                /* Open file fresh each time */
                /* BUG CHECK: Does concurrent open() work? */
                int rfd = open(TEST_FILE, FILE_READ_ONLY);
                if (rfd < 0) {
                    printf("[reader %d] open() failed on iteration %d\n", i, j);
                    continue;
                }
                
                /* Read from start */
                ssize_t n = read(rfd, buf, sizeof(buf) - 1);
                if (n > 0) {
                    buf[n] = '\0';
                    /* BUG CHECK: Is data corrupted by concurrent writers? */
                    /* Look for obvious corruption patterns */
                    int corrupted = 0;
                    for (int k = 0; k < n && !corrupted; k++) {
                        if (buf[k] == '\0' && k < n - 1) {
                            corrupted = 1;
                        }
                    }
                    if (corrupted) {
                        printf("[reader %d] BUG: Data corruption detected!\n", i);
                    }
                }
                
                close(rfd);
                
                /* Small delay */
                for (volatile int k = 0; k < 1000; k++) {
                    __asm__("pause");
                }
            }
            
            printf("[reader %d] Completed\n", i);
            exit(0);
        }
    }
    
    /* Spawn concurrent writers */
    for (int i = 0; i < NUM_WRITERS; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[fs_concurrent] Failed to create writer %d\n", i);
            continue;
        }
        if (pid == 0) {
            /* WRITER process */
            char msg[32];
            
            for (int j = 0; j < 5; j++) {
                /* BUG CHECK: Concurrent open with O_TRUNC? */
                int wfd = open(TEST_FILE, FILE_WRITE_ONLY);
                if (wfd < 0) {
                    printf("[writer %d] open() failed\n", i);
                    continue;
                }
                
                /* Seek to different positions based on writer ID */
                /* BUG CHECK: Does lseek work under concurrent access? */
                lseek(wfd, i * 8, 0);  /* SEEK_SET = 0 */
                
                /* Write our identifier */
                msg[0] = 'W';
                msg[1] = '0' + i;
                msg[2] = ':';
                msg[3] = '0' + j;
                msg[4] = '_';
                msg[5] = '_';
                msg[6] = '_';
                msg[7] = '\0';
                
                write(wfd, msg, 7);
                close(wfd);
                
                /* No delay - maximum contention */
            }
            
            printf("[writer %d] Completed\n", i);
            exit(0);
        }
    }
    
    /* Parent: Test file descriptor inheritance behavior */
    printf("[fs_concurrent] Testing fd inheritance after fork...\n");
    
    int parent_fd = open(TEST_FILE, FILE_READ_ONLY);
    if (parent_fd >= 0) {
        /* Read to move offset */
        char buf[8];
        read(parent_fd, buf, 4);
        
        int inherit_pid = fork();
        if (inherit_pid == 0) {
            /* Child: check if we inherited the fd and offset */
            /* BUG CHECK: Is fd table properly copied? */
            /* BUG CHECK: Is file offset shared or copied? */
            char child_buf[8];
            ssize_t n = read(parent_fd, child_buf, 4);
            if (n > 0) {
                printf("[inherit_child] Read from inherited fd: %.4s\n", child_buf);
            } else {
                printf("[inherit_child] BUG: Cannot read from inherited fd\n");
            }
            exit(0);
        }
        
        close(parent_fd);
    }
    
    /* Wait for children */
    while (waitpid(-1) > 0);
    
    /* Final read to check file state */
    int final_fd = open(TEST_FILE, FILE_WRITE_ONLY);
    if (final_fd >= 0) {
        char final_buf[128];
        ssize_t n = read(final_fd, final_buf, sizeof(final_buf) - 1);
        if (n > 0) {
            final_buf[n] = '\0';
            printf("[fs_concurrent] Final file content: %s\n", final_buf);
        }
        close(final_fd);
    }
    
    printf("[fs_concurrent] Test complete\n");
    return 0;
}
