/**
 * shell.h - Shell types and configuration
 */

#pragma once

#include <stddef.h>

#define MAX_INPUT 256
#define MAX_ARGS  16

/* Command handler function type */
typedef void (*cmd_handler_t)(int argc, char *argv[]);

/* Command definition */
typedef struct {
    const char *name;
    const char *help;
    cmd_handler_t handler;
} command_t;

/* Read a line from stdin, returns length or -1 on error/EOF */
int shell_read_line(char *buf, int max);

/* Parse input line into argv array, returns argc */
int shell_parse_args(char *line, char *argv[]);

/* Find and execute a command, returns 1 if found, 0 if not */
int shell_exec_cmd(const char *name, int argc, char *argv[]);

/* Register the builtin commands */
void shell_register_builtins(void);
