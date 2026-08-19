/**
 * host_stdlib.c - Host test wrapper for stdlib.c
 *
 * Includes stdlib.c with function names prefixed with zenos_
 * to avoid conflicts with host libc.
 */

#define atoi zenos_atoi
#define atol zenos_atol
#define malloc zenos_malloc
#define free zenos_free
#define calloc zenos_calloc
#define realloc zenos_realloc
#define _Exit zenos__Exit
#define _exit zenos__exit

/* Stub out _exit for host testing */
static void zenos__exit(int status) { (void)status; }

/* Include our implementation (minus the _exit call) */
#include "../src/stdlib.c"
