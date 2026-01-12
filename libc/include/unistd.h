#pragma once

#include <sys/types.h>

ssize_t read(int fd, void *buf, size_t count);

ssize_t write(int fd, const void *buf, size_t count);

int close(int fd);

off_t lseek(int fd, off_t offset, int whence);

int fork(void);

int pause(void);

int execve(const char *filename, char *const argv[], char *const envp[]);

void exit(int code);