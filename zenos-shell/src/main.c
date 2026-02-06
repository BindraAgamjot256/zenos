/**
 * main.c - Shell entry point
 */

#include "shell.h"
#include <stdio.h>
#include <unistd.h>

/* ANSI color codes */
#define RESET       "\x1b[0m"
#define BOLD        "\x1b[1m"
#define FG_CYAN     "\x1b[36m"
#define FG_GREEN    "\x1b[32m"
#define FG_YELLOW   "\x1b[33m"
#define FG_RED      "\x1b[31m"
#define FG_BLUE     "\x1b[34m"
#define FG_MAGENTA  "\x1b[35m"

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    
    char input[MAX_INPUT];
    char *args[MAX_ARGS];
    

    printf("\n");
    printf(FG_CYAN "=================================" RESET "\n");
    printf(BOLD FG_GREEN "  Welcome to Zenos Shell v0.1.0" RESET "\n");
    printf(FG_YELLOW "  Type 'help' for commands" RESET "\n");
    printf(FG_CYAN "=================================" RESET "\n");
    printf("\n");
    shell_register_builtins();
    while (1) {
        printf(BOLD FG_BLUE "zenos" FG_MAGENTA "> " RESET);
        
        int len = shell_read_line(input, MAX_INPUT);
        if (len < 0) {
            printf("\n" FG_YELLOW "[shell] EOF received, exiting" RESET "\n");
            break;
        }
        
        if (len == 0) continue;
        
        int nargs = shell_parse_args(input, args);
        if (nargs == 0) continue;
        
        if (!shell_exec_cmd(args[0], nargs, args)) {
            printf(FG_RED "Unknown command: %s, " RESET "\n", args[0]);
            printf(FG_YELLOW "Type 'help' for available commands" RESET "\n");
        }
    }
    
    exit(0);
    return 0;
}
