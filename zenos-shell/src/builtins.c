/**
 * builtins.c - Built-in shell commands
 */

#include "builtins.h"
#include "shell.h"
#include <stdio.h>
#include <unistd.h>

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
    printf("Zenos Shell - Built-in Commands:\n");
    printf("  echo <args>  - Print arguments\n");
    printf("  help         - Show this help\n");
    printf("  clear        - Clear screen\n");
    printf("  version      - Show shell version\n");
    printf("  exit         - Exit the shell\n");
}

void cmd_clear(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    write(1, "\033[2J\033[H", 7);
}

void cmd_version(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    printf("Zenos Shell version 0.1.0\n");
}

void cmd_exit(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    printf("Goodbye!\n");
    exit(0);
}
