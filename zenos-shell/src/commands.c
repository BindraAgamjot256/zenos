/**
 * commands.c - Command registry and execution
 */

#include "shell.h"
#include "builtins.h"
#include <string.h>

#define MAX_COMMANDS 32

static command_t commands[MAX_COMMANDS];
static int num_commands = 0;

/* Register a command */
static void register_cmd(const char *name, const char *help, cmd_handler_t handler) {
    if (num_commands < MAX_COMMANDS) {
        commands[num_commands].name = name;
        commands[num_commands].help = help;
        commands[num_commands].handler = handler;
        num_commands++;
    }
}

void shell_register_builtins(void) {
    register_cmd("echo",    "Print arguments",                 cmd_echo);
    register_cmd("help",    "Show this help",                  cmd_help);
    register_cmd("clear",   "Clear screen",                    cmd_clear);
    register_cmd("version", "Show shell version",              cmd_version);
    register_cmd("exit",    "Exit the shell",                  cmd_exit);
    register_cmd("getpid",  "get the process id of the shell", cmd_getpid);
    register_cmd("path",    "Get or set PATH variable",        cmd_path);
}

int shell_exec_cmd(const char *name, int argc, char *argv[]) {
    for (int i = 0; i < num_commands; i++) {
        if (strcmp(commands[i].name, name) == 0) {
            commands[i].handler(argc, argv);
            return 1;
        }
    }
    return 0;
}
