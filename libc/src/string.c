/**
 * string.c - String and memory manipulation functions
 *
 * Standard C library string functions for Zenos userspace.
 * All functions are implemented from scratch without external dependencies.
 */

#include "stddef.h"  // for size_t
#include "string.h"

/** Calculate length of null-terminated string (not including null) */
size_t strlen(const char *s) {
    size_t len = 0;
    while (*s++) len++;
    return len;
}

/** Copy string src to dest, returns dest */
char *strcpy(char *dest, const char *src) {
    char *d = dest;
    while ((*d++ = *src++)) {
    }
    return dest;
}

/** Copy at most n bytes from src to dest, zero-padding if src is shorter */
char *strncpy(char *dest, const char *src, size_t n) {
    char *d = dest;
    size_t i;
    for (i = 0; i < n && src[i] != '\0'; i++) {
        d[i] = src[i];
    }
    for (; i < n; i++) {
        d[i] = '\0';
    }
    return dest;
}

/** Compare two strings, returns <0, 0, or >0 */
int strcmp(const char *s1, const char *s2) {
    while (*s1 && (*s1 == *s2)) {
        s1++;
        s2++;
    }
    return (unsigned char) *s1 - (unsigned char) *s2;
}

/** Compare at most n bytes of two strings */
int strncmp(const char *s1, const char *s2, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (s1[i] != s2[i] || s1[i] == '\0' || s2[i] == '\0')
            return (unsigned char) s1[i] - (unsigned char) s2[i];
    }
    return 0;
}

/** Append src to end of dest, returns dest */
char *strcat(char *dest, const char *src) {
    char *d = dest;
    while (*d) d++;
    while ((*d++ = *src++)) {
    }
    return dest;
}

/** Append at most n bytes from src to dest */
char *strncat(char *dest, const char *src, size_t n) {
    char *d = dest;
    while (*d) d++;
    size_t i;
    for (i = 0; i < n && src[i]; i++) {
        d[i] = src[i];
    }
    d[i] = '\0';
    return dest;
}

/** Find first occurrence of character c in string s, or NULL */
char *strchr(const char *s, int c) {
    while (*s) {
        if (*s == (char) c) return (char *) s;
        s++;
    }
    return (c == 0) ? (char *) s : NULL;
}

/** Find last occurrence of character c in string s, or NULL */
char *strrchr(const char *s, int c) {
    char *last = NULL;
    while (*s) {
        if (*s == (char) c) last = (char *) s;
        s++;
    }
    return (c == 0) ? (char *) s : last;
}

/** Find first occurrence of substring needle in haystack, or NULL */
char *strstr(const char *haystack, const char *needle) {
    if (!*needle) return (char *) haystack;
    for (; *haystack; haystack++) {
        const char *h = haystack;
        const char *n = needle;
        while (*h && *n && *h == *n) {
            h++;
            n++;
        }
        if (!*n) return (char *) haystack;
    }
    return NULL;
}

/** Fill n bytes of memory with byte c */
void *memset(void *s, int c, size_t n) {
    unsigned char *p = s;
    while (n--) *p++ = (unsigned char) c;
    return s;
}

/** Compare n bytes of memory, returns <0, 0, or >0 */
int memcmp(const void *s1, const void *s2, size_t n) {
    const unsigned char *p1 = s1, *p2 = s2;
    while (n--) {
        if (*p1 != *p2) return (int) (*p1) - (int) (*p2);;
        p1++;
        p2++;
    }
    return 0;
}

/** Copy n bytes from src to dest (undefined if overlapping, use memmove) */
void *memcpy(void *dest, const void *src, size_t n) {
    unsigned char *d = dest;
    const unsigned char *s = src;
    while (n--) *d++ = *s++;
    return dest;
}

/** Copy n bytes from src to dest (safe for overlapping regions) */
void *memmove(void *dest, const void *src, size_t n) {
    unsigned char *d = dest;
    const unsigned char *s = src;
    if (d < s) {
        while (n--) *d++ = *s++;
    } else {
        d += n;
        s += n;
        while (n--) *--d = *--s;
    }
    return dest;
}