/**
 * ls.c - List directory contents (POSIX compliant)
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <dirent.h>

static int list_dir(const char *path, int print_header) {
    DIR *dir = opendir(path);
    if (!dir) {
        printf("ls: cannot access '%s': %d\n", path, errno);
        return 1;
    }

    if (print_header) {
        printf("%s:\n", path);
    }

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        /* POSIX: skip hidden files (starting with '.') by default */
        if (entry->d_name[0] == '.') {
            continue;
        }
        printf("%s\n", entry->d_name);
    }

    closedir(dir);
    return 0;
}

int main(int argc, char *argv[]) {
    int exit_status = 0;

    if (argc <= 1) {
        exit_status = list_dir(".", 0);
    } else if (argc == 2) {
        exit_status = list_dir(argv[1], 0);
    } else {
        for (int i = 1; i < argc; i++) {
            if (i > 1) {
                printf("\n");
            }
            if (list_dir(argv[i], 1) != 0) {
                exit_status = 1;
            }
        }
    }

    return exit_status;
}
