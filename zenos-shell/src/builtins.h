/**
 * builtins.h - Built-in command declarations
 */

#pragma once

void cmd_echo(int argc, char *argv[]);
void cmd_help(int argc, char *argv[]);
void cmd_clear(int argc, char *argv[]);
void cmd_version(int argc, char *argv[]);
void cmd_exit(int argc, char *argv[]);
void cmd_getpid(int argc, char *argv[]);
void cmd_path(int argc, char *argv[]);

/* PATH management */
const char *shell_get_path(void);
void shell_set_path(const char *new_path);