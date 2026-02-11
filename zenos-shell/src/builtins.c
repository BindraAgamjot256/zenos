/**
 * builtins.c - Built-in shell commands
 */

#include "builtins.h"
#include <stdio.h>
#include <unistd.h>

/* ANSI color codes */
#define RESET       "\x1b[0m"
#define BOLD        "\x1b[1m"
#define FG_CYAN     "\x1b[36m"
#define FG_GREEN    "\x1b[32m"
#define FG_YELLOW   "\x1b[33m"
#define FG_WHITE    "\x1b[37m"

void cmd_echo(int argc, char *argv[]) {
    for (int i = 1; i < argc; i++) {
        if (i > 1) printf(" ");
        printf("%s", argv[i]);
    }
    printf("\n");
}

void cmd_help(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    printf(BOLD FG_CYAN "Zenos Shell - Built-in Commands:" RESET "\n");
    printf(FG_GREEN "  echo " FG_WHITE "<args>  " RESET "- Print arguments\n");
    printf(FG_GREEN "  help         " RESET "- Show this help\n");
    printf(FG_GREEN "  clear        " RESET "- Clear screen\n");
    printf(FG_GREEN "  version      " RESET "- Show shell version\n");
    printf(FG_GREEN "  exit         " RESET "- Exit the shell\n");
    printf(FG_GREEN "  getpid       " RESET "- Get the process id of the shell\n");
    printf(FG_GREEN "  path " FG_WHITE "[path]  " RESET "- Get or set PATH variable\n");
}

void cmd_clear(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    write(1, "\033[2J\033[H", 7);
}

void cmd_version(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    printf(FG_CYAN "Zenos Shell version " BOLD "0.1.0" RESET "\n");
}

void cmd_exit(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    printf(FG_YELLOW "Goodbye!" RESET "\n");
    exit(0);
}

void cmd_getpid(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    pid_t pid = getpid();
    pid_t ppid = getppid();
    printf(FG_CYAN "zenos shell running as pid " BOLD "%d" RESET FG_CYAN ", parent pid " BOLD "%d" RESET, pid, ppid);
    if (ppid == 1) printf(FG_GREEN " (init process)" RESET "\n");
    else printf("\n");
}

void cmd_path(int argc, char *argv[]) {
    if (argc == 1) {
        /* Print current PATH */
        printf("%s\n", shell_get_path());
    } else if (argc == 2) {
        /* Set new PATH */
        shell_set_path(argv[1]);
        printf(FG_GREEN "PATH updated to: " RESET "%s\n", argv[1]);
    } else {
        printf(FG_YELLOW "Usage: path [new_path]" RESET "\n");
        printf("  Without arguments: display current PATH\n");
        printf("  With argument: set PATH to the given value\n");
    }
}