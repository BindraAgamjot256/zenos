/**
 * input.c - Input handling
 */

#include <errno.h>
#include <stdio.h>
#include "shell.h"
#include <unistd.h>

int shell_read_line(char *buf, int max) {
    ssize_t n = read(STDIN_FILENO, buf, max - 1);
    if (n == -1) {
        printf("error: %d\n", errno);
        return -1;
    }
    
    /* Remove trailing newline if present */
    while (n > 0 && buf[n-1] == '\n') {
        n--;
    }
    
    buf[n] = '\0';
    return (int)n;
}

int shell_parse_args(char *line, char *argv[]) {
    int argc = 0;
    char *p = line;
    
    while (*p && argc < MAX_ARGS - 1) {
        while (*p == ' ' || *p == '\t') p++;
        if (*p == '\0') break;
        
        argv[argc++] = p;
        
        while (*p && *p != ' ' && *p != '\t') p++;
        if (*p) *p++ = '\0';
    }
    
    argv[argc] = 0;
    return argc;
}
