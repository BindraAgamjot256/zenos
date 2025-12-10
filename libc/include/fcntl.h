#pragma once

int open(const char *pathname, int flags);

#define O_RDONLY 0x0001
#define O_WRONLY 0x0002
#define O_CREAT  0x0004
#define O_TRUNC  0x0008