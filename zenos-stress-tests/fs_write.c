//
// File write stress test for Zenos kernel
//

#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>

#define TEST_FILE "/write_stress.txt"
#define NUM_ITERATIONS 10
#define WRITE_SIZE 64

int main() {
    printf("[fs_write] Starting file write stress test\n");

    for (int i = 0; i < NUM_ITERATIONS; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[fs_write] Fork failed at iteration %d\n", i);
            continue;
        }
        else if (pid == 0) {
            /* Child: write to file */
            int fd = open(TEST_FILE, O_WRONLY | O_CREAT | O_APPEND);
            if (fd < 0) {
                printf("[fs_write] Child %d: open failed, errno: %d\n", i, errno);
                return 1;
            }

            char buf[WRITE_SIZE];
            int len = snprintf(buf, sizeof(buf), "Child %d write\n", i);
            ssize_t written = write(fd, buf, len);
            if (written > 0) {
                printf("[fs_write] Child %d: wrote %ld bytes\n", i, written);
            } else {
                printf("[fs_write] Child %d: write failed\n", i);
            }
            close(fd);
            return 0;
        }

        /* Parent: also write to the same file */
        int fd = open(TEST_FILE, O_WRONLY | O_CREAT | O_APPEND);
        if (fd < 0) {
            printf("[fs_write] Parent: open failed at iteration %d\n", i);
            continue;
        }

        char buf[WRITE_SIZE];
        int len = snprintf(buf, sizeof(buf), "Parent iteration %d write\n", i);
        ssize_t written = write(fd, buf, len);
        if (written > 0) {
            printf("[fs_write] Parent: wrote %ld bytes (iter %d)\n", written, i);
        } else {
            printf("[fs_write] Parent: write failed (iter %d)\n", i);
        }
        close(fd);
    }

    /* Wait for all children */
    while (waitpid(-1, NULL, 0) > 0);

    /* Verify by reading back */
    printf("[fs_write] Reading back file contents:\n");
    int fd = open(TEST_FILE, O_RDONLY);
    if (fd >= 0) {
        char buf[1024];
        ssize_t len = read(fd, buf, sizeof(buf) - 1);
        if (len > 0) {
            buf[len] = '\0';
            printf("%s", buf);
        }
        close(fd);
    } else {
        printf("[fs_write] Could not open file for reading\n");
    }

    printf("[fs_write] Test complete\n");
    return 0;
}
