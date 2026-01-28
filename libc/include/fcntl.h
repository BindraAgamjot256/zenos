#pragma once
#include <stdint.h>



/* The bitmask type */
typedef uint64_t FileOpenOptions;

/* Access modes (mutually exclusive, lowest bits just like O_* flags) */
#define FILE_READ_ONLY   ((FileOpenOptions)0)  /* O_RDONLY */
#define FILE_WRITE_ONLY  ((FileOpenOptions)1)  /* O_WRONLY */
#define FILE_READ_WRITE  ((FileOpenOptions)2)  /* O_RDWR  */

/* Flags */
#define FILE_CREATE        ((FileOpenOptions)0100)       /* O_CREAT */
#define FILE_EXCLUSIVE     ((FileOpenOptions)0200)       /* O_EXCL */
#define FILE_NOCTTY        ((FileOpenOptions)0400)       /* O_NOCTTY */
#define FILE_TRUNCATE      ((FileOpenOptions)01000)      /* O_TRUNC */
#define FILE_APPEND        ((FileOpenOptions)02000)      /* O_APPEND */
#define FILE_NONBLOCK      ((FileOpenOptions)04000)      /* O_NONBLOCK */
#define FILE_SYNC          ((FileOpenOptions)010000)     /* O_SYNC */
#define FILE_CLOSE_ON_EXEC ((FileOpenOptions)02000000)   /* O_CLOEXEC */

/* Helper macros (because C loves ceremony) */
#define FILE_OPTIONS_HAS(opts, flag) (((opts) & (flag)) != 0)
#define FILE_OPTIONS_ADD(opts, flag) ((opts) |= (flag))
#define FILE_OPTIONS_REMOVE(opts, flag) ((opts) &= ~(flag))

int open(const char *pathname, FileOpenOptions flags);