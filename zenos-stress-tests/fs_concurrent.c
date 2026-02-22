//
// Created by Agamjot Singh Bindra on 21/02/26.
//

#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>


int main() {
    /* Spawn multiple processes that read the same file concurrently */
    for (int i = 0; i < 50; i++) {
        int pid = fork();
        if (pid < 0) {
            printf("[fs_concurrent] Fork failed at iteration %d\n", i);
            continue;
        }
        else if (pid == 0) {
            /* Child: perform concurrent read */
            int fd = open("/chksum.txt", O_RDONLY);
            char buf[256];
            ssize_t len = read(fd, buf, sizeof(buf));
            if (len > 0) {
                printf("[fs_concurrent] readL %s\n", buf);
            } else printf("[fs_concurrent] Read failed in child process\n");
            close(fd);
            return 0;
        }
        /* Parent: also read the same file */
        int fd = open("/chksum.txt", O_RDONLY);
        char buf[256];
        ssize_t len = read(fd, buf, sizeof(buf));
        if (len > 0) {
            printf("[fs_concurrent] readP %s\n", buf);
        } else printf("[fs_concurrent] Read failed in parent process\n");
        close(fd);

        /* Parent waits for children to finish */
        while (waitpid(-1, NULL, 0) > 0);
    }
    return 0;
}
