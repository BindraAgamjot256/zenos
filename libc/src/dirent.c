/**
 * dirent.c - Directory entry operations
 *
 * Provides opendir, readdir, closedir using getdents syscall.
 * Uses a static DIR pool to avoid malloc/free.
 */

#include "dirent.h"
#include "sys/syscall.h"
#include "fcntl.h"
#include "unistd.h"
#include "errno.h"

#define MAX_OPEN_DIRS 16

static DIR dir_pool[MAX_OPEN_DIRS];
static int dir_used[MAX_OPEN_DIRS];

static inline long syscall_ret(long ret) {
    if (ret < 0 && ret > -4096) {
        errno = (int)(-ret);
        return -1;
    }
    return ret;
}

static DIR *alloc_dir(void) {
    for (int i = 0; i < MAX_OPEN_DIRS; i++) {
        if (!dir_used[i]) {
            dir_used[i] = 1;
            return &dir_pool[i];
        }
    }
    return NULL;
}

static void free_dir(DIR *dirp) {
    for (int i = 0; i < MAX_OPEN_DIRS; i++) {
        if (&dir_pool[i] == dirp) {
            dir_used[i] = 0;
            return;
        }
    }
}

DIR *opendir(const char *name) {
    int fd = open(name, O_RDONLY | O_DIRECTORY);
    if (fd < 0) {
        return NULL;
    }
    return fdopendir(fd);
}

DIR *fdopendir(int fd) {
    DIR *dirp = alloc_dir();
    if (!dirp) {
        close(fd);
        errno = EMFILE;
        return NULL;
    }
    dirp->fd = fd;
    dirp->buf_pos = 0;
    dirp->buf_end = 0;
    return dirp;
}

struct dirent *readdir(DIR *dirp) {
    if (!dirp) {
        errno = EBADF;
        return NULL;
    }

    if (dirp->buf_pos >= dirp->buf_end) {
        long ret = syscall3(SYS_getdents, dirp->fd, (long)dirp->buf, sizeof(dirp->buf));
        if (ret <= 0) {
            if (ret < 0) {
                syscall_ret(ret);
            }
            return NULL;
        }
        dirp->buf_pos = 0;
        dirp->buf_end = (size_t)ret;
    }

    struct dirent *entry = (struct dirent *)(dirp->buf + dirp->buf_pos);
    dirp->buf_pos += entry->d_reclen;
    return entry;
}

int closedir(DIR *dirp) {
    if (!dirp) {
        errno = EBADF;
        return -1;
    }
    int ret = close(dirp->fd);
    free_dir(dirp);
    return ret;
}

void rewinddir(DIR *dirp) {
    if (dirp) {
        lseek(dirp->fd, 0, SEEK_SET);
        dirp->buf_pos = 0;
        dirp->buf_end = 0;
    }
}
