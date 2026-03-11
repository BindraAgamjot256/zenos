/**
 * od.c - Octal dump (and other formats)
 */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>

#define BUF_SIZE 16

static int format = 'o';  /* Default: octal */
static int show_addr = 1;

static void print_address(long offset) {
    if (show_addr) {
        printf("%07lo ", offset);
    }
}

static void dump_octal(unsigned char *buf, ssize_t n) {
    for (ssize_t i = 0; i < n; i++) {
        printf(" %03o", buf[i]);
    }
}

static void dump_hex(unsigned char *buf, ssize_t n) {
    for (ssize_t i = 0; i < n; i++) {
        printf(" %02x", buf[i]);
    }
}

static void dump_decimal(unsigned char *buf, ssize_t n) {
    for (ssize_t i = 0; i < n; i++) {
        printf(" %3d", buf[i]);
    }
}

static void dump_char(unsigned char *buf, ssize_t n) {
    for (ssize_t i = 0; i < n; i++) {
        unsigned char c = buf[i];
        if (c == '\0') {
            printf("  \\0");
        } else if (c == '\n') {
            printf("  \\n");
        } else if (c == '\r') {
            printf("  \\r");
        } else if (c == '\t') {
            printf("  \\t");
        } else if (c == '\\') {
            printf("  \\\\");
        } else if (c >= 32 && c < 127) {
            printf("   %c", c);
        } else {
            printf(" %03o", c);
        }
    }
}

static void dump_line(unsigned char *buf, ssize_t n) {
    switch (format) {
        case 'x':
            dump_hex(buf, n);
            break;
        case 'd':
            dump_decimal(buf, n);
            break;
        case 'c':
            dump_char(buf, n);
            break;
        case 'o':
        default:
            dump_octal(buf, n);
            break;
    }
}

static int dump_file(int fd) {
    unsigned char buf[BUF_SIZE];
    ssize_t n;
    long offset = 0;

    while ((n = read(fd, buf, BUF_SIZE)) > 0) {
        print_address(offset);
        dump_line(buf, n);
        printf("\n");
        offset += n;
    }

    if (n < 0) {
        return errno;
    }

    print_address(offset);
    printf("\n");
    return 0;
}

static void usage(void) {
    printf("Usage: od [-A addr_format] [-t type] [file...]\n");
    printf("  -A x|d|o|n  Address format (hex, decimal, octal, none)\n");
    printf("  -t o|x|d|c  Output type (octal, hex, decimal, char)\n");
    printf("  -x          Hex output (same as -t x)\n");
    printf("  -c          Character output (same as -t c)\n");
}

int main(int argc, char *argv[]) {
    int i;
    int file_start = 1;

    for (i = 1; i < argc && argv[i][0] == '-'; i++) {
        if (argv[i][1] == 'A' && argv[i][2] == '\0') {
            if (i + 1 >= argc) {
                usage();
                return 1;
            }
            i++;
            switch (argv[i][0]) {
                case 'x': case 'd': case 'o':
                    break;
                case 'n':
                    show_addr = 0;
                    break;
                default:
                    usage();
                    return 1;
            }
        } else if (argv[i][1] == 't' && argv[i][2] == '\0') {
            if (i + 1 >= argc) {
                usage();
                return 1;
            }
            i++;
            format = argv[i][0];
        } else if (argv[i][1] == 'x' && argv[i][2] == '\0') {
            format = 'x';
        } else if (argv[i][1] == 'c' && argv[i][2] == '\0') {
            format = 'c';
        } else if (argv[i][1] == '-' && argv[i][2] == '\0') {
            i++;
            break;
        } else {
            usage();
            return 1;
        }
    }
    file_start = i;

    if (file_start >= argc) {
        return dump_file(STDIN_FILENO);
    }

    for (i = file_start; i < argc; i++) {
        /* Check if file exists and is readable using access syscall */
        if (access(argv[i], R_OK) != 0) {
            printf("od: %s: No such file or directory\n", argv[i]);
            return 1;
        }
        
        int fd = open(argv[i], O_RDONLY);
        if (fd < 0) {
            printf("od: %s: No such file or directory\n", argv[i]);
            return errno;
        }

        int ret = dump_file(fd);
        close(fd);
        if (ret != 0) {
            return ret;
        }
    }

    return 0;
}
