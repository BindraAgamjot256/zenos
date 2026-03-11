/**
 * external.c - External command execution with PATH lookup
 */

#include "shell.h"
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/wait.h>

#define PATH_MAX 1024

extern char **environ;

static char current_path[PATH_MAX];
/* Initialize PATH from environment */
void shell_init_path(void) {
    char *env_path = getenv("PATH");
    if (env_path) {
        strncpy(current_path, env_path, PATH_MAX - 1);
        current_path[PATH_MAX - 1] = '\0';
    }
}

/* Try to execute a command at the given path */
static int try_exec(const char *path, char *argv[]) {
    pid_t pid = fork();
    
    if (pid < 0) {
        exit(errno);
    }
    
    if (pid == 0) {
        /* Child process - close stdin to prevent keyboard buffer conflicts */
        extern char **environ;
        execve(path, argv, environ);
        /* If execve returns, it failed */
        _exit(errno);
    }
    
    /* Parent process */
    int status;
    if (waitpid(pid, &status, 0) < 0) {
        return 0;
    }
    
    return 1;
}

static int try_exec_path(const char *name, char *argv[]) {
    pid_t pid = fork();

    if (pid < 0) {
        exit(errno);
    }

    if (pid == 0) {
        /* Child process - close stdin to prevent keyboard buffer conflicts */
        execvp(name, argv);
        /* If execve returns, it failed */
        _exit(-1);
    }

    /* Parent process */
    int ret = waitpid(pid, NULL, 0);

    if (ret == -1) {
        return 0;
    }
    return 1;
}

/* Execute external command by searching PATH */
int shell_exec_external(const char *name, int argc, char *argv[]) {
    (void)argc;
    
    /* If command contains a slash, try it directly */
    if (strchr(name, '/')) {
        if (access(name, X_OK) == 0) {
            return try_exec(name, argv);
        }
        return 0;
    }
    
    /* Search PATH */
    return try_exec_path(name, argv);
}

/* Get current PATH */
const char *shell_get_path(void) {
    return current_path;
}

/* Set current PATH */
void shell_set_path(const char *new_path) {
    strncpy(current_path, new_path, PATH_MAX - 1);
    current_path[PATH_MAX - 1] = '\0';
}
