/**
 * cat.c - Concatenate and print files
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

#define BUF_SIZE 4096

int main(int argc, char *argv[]) {
    char buf[BUF_SIZE];
    int fd;
    ssize_t n;
    
    if (argc < 2) {
        printf("Usage: cat <file> [file...]\n");
        return 1;
    }
    
    for (int i = 1; i < argc; i++) {
        fd = open(argv[i], O_RDONLY);
        if (fd < 0) {
            printf("cat: %s: No such file or directory{error: %d}\n", argv[i], errno);
            exit(errno);
        }
        
        while ((n = read(fd, buf, BUF_SIZE)) > 0) {
            ssize_t written = 0;
            while (written < n) {
                ssize_t ret = write(STDOUT_FILENO, buf + written, n - written);
                if (ret < 0) {
                    close(fd);
                    exit(errno);
                }
                written += ret;
            }
        }
        
        if (n < 0) {
            close(fd);
            exit(errno);
        }
        
        if (close(fd) < 0) {
            exit(errno);
        }
    }
    
    return 0;
}
