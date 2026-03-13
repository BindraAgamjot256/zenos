#pragma once

#include "sys/types.h"
#include "stddef.h"

#ifdef __cplusplus
extern "C" {
#endif

#define DT_UNKNOWN  0
#define DT_FIFO     1
#define DT_CHR      2
#define DT_DIR      4
#define DT_BLK      6
#define DT_REG      8
#define DT_LNK      10
#define DT_SOCK     12

struct dirent {
    ino_t  d_ino;
    off_t  d_off;
    unsigned short d_reclen;
    unsigned char  d_type;
    char   d_name[256];
};

typedef struct {
    int fd;
    char buf[4096];
    size_t buf_pos;
    size_t buf_end;
} DIR;

DIR *opendir(const char *name);
DIR *fdopendir(int fd);
struct dirent *readdir(DIR *dirp);
int closedir(DIR *dirp);
void rewinddir(DIR *dirp);

#ifdef __cplusplus
}
#endif
