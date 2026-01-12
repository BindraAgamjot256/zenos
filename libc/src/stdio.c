/**
 * stdio.c - Standard I/O for Zenos
 *
 * Implements printf() with basic format specifiers. All output goes directly
 * to stdout (fd 1) via the write() syscall—no buffering.
 *
 * Supported format specifiers:
 *   %d  - signed decimal integer
 *   %u  - unsigned decimal integer
 *   %x  - unsigned hexadecimal (lowercase)
 *   %p  - pointer (prints as 0x...)
 *   %s  - null-terminated string
 *   %c  - single character
 *   %%  - literal percent sign
 *
 * Width and zero-padding supported (e.g., %08x for zero-padded hex).
 */

#include "unistd.h"
#include "stdarg.h"
#include "stdint.h"
#include "string.h"

/** Write a single character to stdout, returns 1 */
static int write_char(char c) {
    write(1, &c, 1);
    return 1;
}

/** Write a null-terminated string to stdout, returns bytes written */
static int write_str(const char *s) {
    const int len = (int) strlen(s);
    write(1, s, len);
    return len;
}

/** Print unsigned int in given base (10 or 16) with optional width/zero-padding */
static int print_uint_base(unsigned int value, int base, int width, int zero_pad) {
    char buffer[32];
    int i = 0;
    int bytes = 0;

    if (value == 0) {
        buffer[i++] = '0';
    } else {
        while (value > 0) {
            unsigned int digit = value % (unsigned) base;
            if (digit < 10)
                buffer[i++] = (char) ('0' + digit);
            else
                buffer[i++] = (char) ('a' + (digit - 10));
            value /= (unsigned) base;
        }
    }

    int pad_len = width - i;
    char pad_char = zero_pad ? '0' : ' ';

    while (pad_len-- > 0) {
        bytes += write_char(pad_char);
    }

    while (i > 0) {
        bytes += write_char(buffer[--i]);
    }

    return bytes;
}

/** Print signed int with optional width/zero-padding */
static int print_int(int value, int width, int zero_pad) {
    unsigned int u;
    int bytes = 0;
    int is_neg = (value < 0);

    if (is_neg) {
        u = (unsigned int) (-(value + 1)) + 1;
    } else {
        u = (unsigned int) value;
    }

    int digit_count = 0;
    unsigned int tmp = u;
    if (tmp == 0) digit_count = 1;
    else {
        while (tmp > 0) {
            digit_count++;
            tmp /= 10;
        }
    }

    if (is_neg) {
        bytes += write_char('-');
    }

    int total_width = digit_count;
    int pad_len = width - total_width;
    char pad_char = zero_pad ? '0' : ' ';

    while (pad_len-- > 0) {
        bytes += write_char(pad_char);
    }

    bytes += print_uint_base(u, 10, 0, 0);
    return bytes;
}

/** Print pointer as "0x..." hex address */
static int print_pointer(void *ptr) {
    uintptr_t p = (uintptr_t) ptr;
    int bytes = 0;

    bytes += write_str("0x");
    bytes += print_uint_base((unsigned int) p, 16, 0, 0);
    return bytes;
}

/**
 * Formatted output to stdout.
 * @param format  Format string with % specifiers
 * @param ...     Arguments corresponding to format specifiers
 * @return        Number of bytes written
 */
int printf(const char *format, ...) {
    va_list args;
    va_start(args, format);

    int bytes = 0;

    while (*format) {
        if (*format != '%') {
            bytes += write_char(*format++);
            continue;
        }

        format++; // skip '%'

        int zero_pad = 0;
        int width = 0;

        if (*format == '0') {
            zero_pad = 1;
            format++;
        }

        // parse width
        while (*format >= '0' && *format <= '9') {
            width = width * 10 + (*format - '0');
            format++;
        }

        if (*format == '\0') break;

        switch (*format) {
            case 'd': {
                int v = va_arg(args, int);
                bytes += print_int(v, width, zero_pad);
                break;
            }
            case 'u': {
                unsigned int v = va_arg(args, unsigned int);
                bytes += print_uint_base(v, 10, width, zero_pad);
                break;
            }
            case 'x': {
                unsigned int v = va_arg(args, unsigned int);
                bytes += print_uint_base(v, 16, width, zero_pad);
                break;
            }
            case 'p': {
                void *p = va_arg(args, void *);
                bytes += print_pointer(p);
                break;
            }
            case 's': {
                char *s = va_arg(args, char *);
                if (!s) s = "(null)";
                bytes += write_str(s);
                break;
            }
            case 'c': {
                int c = va_arg(args, int);
                bytes += write_char((char) c);
                break;
            }
            case '%': {
                bytes += write_char('%');
                break;
            }
            default: {
                bytes += write_char('%');
                bytes += write_char(*format);
                break;
            }
        }

        format++;
    }

    va_end(args);
    return bytes;
}