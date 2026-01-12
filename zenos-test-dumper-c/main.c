#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>


int main(int argc, char *argv[]) {
    // Display command line arguments
    printf("=== Command Line Arguments ===\n");
    printf("argc = %d\n", argc);

    for (int i = 0; i < argc; i++) {
        if (argv[i] != 0) {
            printf("argv[%d] = \"%s\"\n", i, argv[i]);
        } else {
            printf("argv[%d] = (null)\n", i);
        }
    }
    printf("==============================\n");
    printf("\n");

    // List of files we want to dump
    const char *files[] = {
        "/proc/cpuinfo",
        "/proc/meminfo",
        "/proc/version",
        "/proc/uptime",
        "/proc/1/cmdline",
        "/proc/1/status",
        "/proc/1/stat",
    };
    int num_files = sizeof(files) / sizeof(files[0]);

    char buf[1024];

    for (int i = 0; i < num_files; i++) {
        int fd = open(files[i], O_RDONLY | O_CREAT);
        if (fd < 0) {
            printf("Failed to open %s\n", files[i]);
            continue;
        }

        printf("--- %s ---\n", files[i]);

        ssize_t n;
        while ((n = read(fd, buf, sizeof(buf) - 1)) > 0) {
            buf[n] = '\0';
            printf("%s", buf);
        }

        close(fd);
        printf("\n");
    }

    exit(0);
    return 0;
}
