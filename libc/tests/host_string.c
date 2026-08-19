/**
 * host_string.c - Host test wrapper for string.c
 *
 * Includes string.c with function names prefixed with zenos_
 * to avoid conflicts with host libc.
 */

#define strlen zenos_strlen
#define strcpy zenos_strcpy
#define strncpy zenos_strncpy
#define strcmp zenos_strcmp
#define strncmp zenos_strncmp
#define strcat zenos_strcat
#define strncat zenos_strncat
#define strchr zenos_strchr
#define strrchr zenos_strrchr
#define strstr zenos_strstr
#define strtok zenos_strtok
#define memset zenos_memset
#define memcmp zenos_memcmp
#define memcpy zenos_memcpy
#define memmove zenos_memmove

/* Include our implementation */
#include "../src/string.c"
