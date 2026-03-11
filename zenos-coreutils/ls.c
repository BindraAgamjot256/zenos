/**
 * ls.c - List directory contents
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    const char *path = (argc > 1) ? argv[1] : ".";
    
    /* Check if path exists using access syscall */
    if (access(path, F_OK) != 0) {
        printf("ls: cannot access '%s': No such file or directory\n", path);
        return 1;
    }
    
    /* For now, just confirm the path exists since readdir is not available */
    printf("ls: '%s' exists (directory listing not yet implemented - readdir syscall not available)\n", path);
    
    return 0;
}
