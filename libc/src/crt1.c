/**
 * crt1.c - C runtime initialization for Zenos
 *
 * Stores the environment pointer and provides getenv() functionality.
 * Called by crt0.asm before main().
 */

#include "stdlib.h"
#include "string.h"

/* Global environment pointer - set by __libc_init before main() */
char **environ = NULL;

/**
 * Initialize the C library.
 * Called by crt0.asm with the environment pointer from the stack.
 */
void __libc_init(char **envp) {
    environ = envp;
}

/**
 * Get an environment variable by name.
 * @param name  The name of the environment variable
 * @return      Pointer to the value string, or NULL if not found
 */
char *getenv(const char *name) {
    if (name == NULL || environ == NULL) {
        return NULL;
    }

    size_t name_len = strlen(name);

    for (char **env = environ; *env != NULL; env++) {
        /* Check if this entry starts with "name=" */
        if (strncmp(*env, name, name_len) == 0 && (*env)[name_len] == '=') {
            return &(*env)[name_len + 1];
        }
    }

    return NULL;
}
