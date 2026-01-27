/**
 * main.c - Shell entry point
 */

#include "shell.h"
#include <stdio.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    
    char input[MAX_INPUT];
    char *args[MAX_ARGS];
    
    shell_register_builtins();
    
    printf("\n");
    printf("=================================\n");
    printf("  Welcome to Zenos Shell v0.1.0\n");
    printf("  Type 'help' for commands\n");
    printf("=================================\n");
    printf("\n");
    
    while (1) {
        printf("zenos> ");
        
        int len = shell_read_line(input, MAX_INPUT);
        if (len < 0) {
            printf("\n[shell] EOF received, exiting\n");
            break;
        }
        
        if (len == 0) continue;
        
        int nargs = shell_parse_args(input, args);
        if (nargs == 0) continue;
        
        if (!shell_exec_cmd(args[0], nargs, args)) {
            printf("Unknown command: %s\n", args[0]);
            printf("Type 'help' for available commands\n");
        }
    }
    
    exit(0);
    return 0;
}
