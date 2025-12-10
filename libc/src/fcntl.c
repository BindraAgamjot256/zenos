#include "fcntl.h"
#include "sys/syscall.h"


int open(const char *pathname, const int flags) {
    return (int)syscall2(SYS_open, (long)pathname, (long)flags);
}
