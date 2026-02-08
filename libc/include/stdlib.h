#pragma once

#include "stddef.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Environment access */
extern char **environ;
char *getenv(const char *name);

/* Process termination */
void exit(int status);
void _Exit(int status);

/* Integer conversion */
int atoi(const char *str);
long atol(const char *str);

/* Memory allocation (stubs - not yet implemented) */
void *malloc(size_t size);
void free(void *ptr);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);

#ifdef __cplusplus
}
#endif
