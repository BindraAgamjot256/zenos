/**
 * wc.c - Count lines, words, and bytes
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

#define BUF_SIZE 4096

static void count_file(int fd, const char *filename, long *lines, long *words, long *bytes) {
    char buf[BUF_SIZE];
    ssize_t n;
    int in_word = 0;
    
    *lines = 0;
    *words = 0;
    *bytes = 0;
    
    while ((n = read(fd, buf, BUF_SIZE)) > 0) {
        *bytes += n;
        
        for (ssize_t i = 0; i < n; i++) {
            if (buf[i] == '\n') {
                (*lines)++;
            }
            
            if (buf[i] == ' ' || buf[i] == '\t' || buf[i] == '\n') {
                in_word = 0;
            } else if (!in_word) {
                in_word = 1;
                (*words)++;
            }
        }
    }
    
    if (n < 0) {
        exit(errno);
    }
    
    printf("%ld %ld %ld", *lines, *words, *bytes);
    if (filename) {
        printf(" %s", filename);
    }
    printf("\n");
}

int main(int argc, char *argv[]) {
    long lines, words, bytes;
    long total_lines = 0, total_words = 0, total_bytes = 0;
    
    if (argc == 1) {
        count_file(STDIN_FILENO, NULL, &lines, &words, &bytes);
    } else {
        for (int i = 1; i < argc; i++) {
            int fd = open(argv[i], O_RDONLY);
            if (fd < 0) {
                exit(errno);
            }
            
            count_file(fd, argv[i], &lines, &words, &bytes);
            total_lines += lines;
            total_words += words;
            total_bytes += bytes;
            
            if (close(fd) < 0) {
                exit(errno);
            }
        }
        
        if (argc > 2) {
            printf("%ld %ld %ld total\n", total_lines, total_words, total_bytes);
        }
    }
    
    return 0;
}
