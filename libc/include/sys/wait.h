#pragma once

#include "sys/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Wait options */
#define WNOHANG    1
#define WUNTRACED  2
#define WCONTINUED 8

/* Status macros */
#define WIFEXITED(status)   (((status) & 0x7f) == 0)
#define WEXITSTATUS(status) (((status) & 0xff00) >> 8)
#define WIFSIGNALED(status) (((status) & 0x7f) != 0 && ((status) & 0x7f) != 0x7f)
#define WTERMSIG(status)    ((status) & 0x7f)
#define WIFSTOPPED(status)  (((status) & 0xff) == 0x7f)
#define WSTOPSIG(status)    WEXITSTATUS(status)
#define WIFCONTINUED(status) ((status) == 0xffff)

/* Wait functions */
pid_t wait(int *status);
pid_t waitpid(pid_t pid, int *status, int options);

#ifdef __cplusplus
}
#endif
