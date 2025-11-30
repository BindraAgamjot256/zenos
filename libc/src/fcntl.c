#include <fcntl.h>
#include <string.h>
#include <sys/syscall.h>

extern long syscall3(long n, long a1, long a2, long a3);

int open(const char *pathname, int flags) {
    size_t len = strlen(pathname);
    return (int)syscall3(SYS_open, (long)pathname, (long)len, (long)flags);
}
