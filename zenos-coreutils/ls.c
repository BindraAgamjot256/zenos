/**
 * ls.c - List directory contents
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    
    /* For now, just print a placeholder since readdir is not available */
    printf("ls: directory listing not yet implemented (readdir syscall not available)\n");
    
    return 0;
}
