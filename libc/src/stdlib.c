/**
 * stdlib.c - Standard library functions
 *
 * Provides utility functions like atoi, memory allocation stubs, etc.
 */

#include "stdlib.h"
#include "unistd.h"

/**
 * Convert string to integer.
 */
int atoi(const char *str) {
    int result = 0;
    int sign = 1;

    /* Skip whitespace */
    while (*str == ' ' || *str == '\t' || *str == '\n' ||
           *str == '\r' || *str == '\f' || *str == '\v') {
        str++;
    }

    /* Handle sign */
    if (*str == '-') {
        sign = -1;
        str++;
    } else if (*str == '+') {
        str++;
    }

    /* Convert digits */
    while (*str >= '0' && *str <= '9') {
        result = result * 10 + (*str - '0');
        str++;
    }

    return sign * result;
}

/**
 * Convert string to long.
 */
long atol(const char *str) {
    long result = 0;
    int sign = 1;

    /* Skip whitespace */
    while (*str == ' ' || *str == '\t' || *str == '\n' ||
           *str == '\r' || *str == '\f' || *str == '\v') {
        str++;
    }

    /* Handle sign */
    if (*str == '-') {
        sign = -1;
        str++;
    } else if (*str == '+') {
        str++;
    }

    /* Convert digits */
    while (*str >= '0' && *str <= '9') {
        result = result * 10 + (*str - '0');
        str++;
    }

    return sign * result;
}

/**
 * _Exit - immediate process termination.
 */
void _Exit(int status) {
    _exit(status);
}

/* Memory allocation stubs - not implemented yet */

void *malloc(size_t size) {
    (void)size;
    return (void *)0;  /* Not implemented */
}

void free(void *ptr) {
    (void)ptr;
    /* Not implemented */
}

void *calloc(size_t nmemb, size_t size) {
    (void)nmemb;
    (void)size;
    return (void *)0;  /* Not implemented */
}

void *realloc(void *ptr, size_t size) {
    (void)ptr;
    (void)size;
    return (void *)0;  /* Not implemented */
}
