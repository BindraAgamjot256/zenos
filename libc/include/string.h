#pragma once
#include "stddef.h"

#ifdef __cplusplus
extern "C" {
#endif

size_t strlen(const char *s);

char *strcpy(char *dest, const char *src);

int strcmp(const char *s1, const char *s2);

char *strcat(char *dest, const char *src);

void *memcpy(void *dest, const void *src, size_t n);

void *memmove(void *dest, const void *src, size_t n);

void *memset(void *s, int c, size_t n);

int memcmp(const void *s1, const void *s2, size_t n);

int strncmp(const char *s1, const char *s2, size_t n);

char *strchr(const char *s, int c);

char *strncpy(char *dest, const char *src, size_t n);

char *strncat(char *dest, const char *src, size_t n);

char *strrchr(const char *s, int c);

char *strstr(const char *haystack, const char *needle);

#ifdef __cplusplus
}
#endif