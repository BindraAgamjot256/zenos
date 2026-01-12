/**
 * fcntl.c - File control operations
 *
 * Provides open() for opening files with specified flags.
 */

#include "fcntl.h"
#include "sys/syscall.h"


/**
 * Open a file.
 * @param pathname  Path to the file
 * @param flags     O_RDONLY, O_WRONLY, O_CREAT, O_TRUNC (can be OR'd)
 * @return          File descriptor on success, -1 on error
 */
int open(const char *pathname, const int flags) {
    return (int)syscall2(SYS_open, (long)pathname, (long)flags);
}
