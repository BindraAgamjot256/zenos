/**
 * errno.c - Error number support
 *
 * Provides the errno variable and helper functions for error handling.
 */

#include "errno.h"

/* Global errno variable */
int errno = 0;

/**
 * Get pointer to errno (for thread-local support in the future).
 */
int *__errno_location(void) {
    return &errno;
}
