/**
 * yes.c - Output a string repeatedly until killed
 */

#include <stdio.h>
#include <stdlib.h>

int main(int argc, char *argv[]) {
    const char *msg = (argc > 1) ? argv[1] : "y";
    
    while (1) {
        printf("%s\n", msg);
    }
    
    return 0;
}
