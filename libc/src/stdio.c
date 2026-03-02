/**
 * stdio.c - Standard I/O for Zenos
 *
 * printf() implemented using vsnprintf().
 * All formatting logic lives in vsnprintf().
 * printf() writes the formatted buffer to stdout (fd 1).
 *
 * Supported format specifiers:
 *   %d  - signed decimal integer
 *   %ld - signed decimal long
 *   %u  - unsigned decimal integer
 *   %x  - unsigned hexadecimal (lowercase)
 *   %p  - pointer (prints as 0x...)
 *   %s  - null-terminated string
 *   %c  - single character
 *   %%  - literal percent sign
 *
 * Width and zero-padding supported (e.g., %08x).
 */

#include "unistd.h"
#include "stdarg.h"
#include "stdint.h"

/* ========================= INTERNAL BUFFER WRITER ========================= */

typedef struct {
    char *buf;
    size_t size;
    size_t pos;
    int total;
} sn_buf_t;

static void sn_write_char(sn_buf_t *b, char c) {
    if (b->pos + 1 < b->size) {
        b->buf[b->pos] = c;
    }
    b->pos++;
    b->total++;
}

static void sn_write_str(sn_buf_t *b, const char *s) {
    while (*s) {
        sn_write_char(b, *s++);
    }
}

/* ========================= NUMBER PRINTING ========================= */

static void sn_print_uint_base(sn_buf_t *b,
                               unsigned int value,
                               int base,
                               int width,
                               int zero_pad) {
    char buffer[32];
    int i = 0;

    if (value == 0) {
        buffer[i++] = '0';
    } else {
        while (value > 0) {
            unsigned int digit = value % (unsigned)base;
            if (digit < 10)
                buffer[i++] = (char)('0' + digit);
            else
                buffer[i++] = (char)('a' + (digit - 10));
            value /= (unsigned)base;
        }
    }

    int pad_len = width - i;
    char pad_char = zero_pad ? '0' : ' ';

    while (pad_len-- > 0)
        sn_write_char(b, pad_char);

    while (i > 0)
        sn_write_char(b, buffer[--i]);
}

static void sn_print_int(sn_buf_t *b, int value, int width, int zero_pad) {
    unsigned int u;
    int is_neg = (value < 0);

    if (is_neg)
        u = (unsigned int)(-(value + 1)) + 1;
    else
        u = (unsigned int)value;

    int digit_count = 0;
    unsigned int tmp = u;

    if (tmp == 0)
        digit_count = 1;
    else {
        while (tmp > 0) {
            digit_count++;
            tmp /= 10;
        }
    }

    if (is_neg)
        sn_write_char(b, '-');

    int pad_len = width - digit_count;
    char pad_char = zero_pad ? '0' : ' ';

    while (pad_len-- > 0)
        sn_write_char(b, pad_char);

    sn_print_uint_base(b, u, 10, 0, 0);
}

static void sn_print_long(sn_buf_t *b, long value, int width, int zero_pad) {
    unsigned long u;
    int is_neg = (value < 0);

    if (is_neg)
        u = (unsigned long)(-(value + 1)) + 1;
    else
        u = (unsigned long)value;

    char buffer[32];
    int i = 0;

    if (u == 0) {
        buffer[i++] = '0';
    } else {
        while (u > 0) {
            buffer[i++] = (char)('0' + (u % 10));
            u /= 10;
        }
    }

    if (is_neg)
        sn_write_char(b, '-');

    int pad_len = width - i;
    char pad_char = zero_pad ? '0' : ' ';

    while (pad_len-- > 0)
        sn_write_char(b, pad_char);

    while (i > 0)
        sn_write_char(b, buffer[--i]);
}

static void sn_print_pointer(sn_buf_t *b, void *ptr) {
    uintptr_t p = (uintptr_t)ptr;
    sn_write_str(b, "0x");
    sn_print_uint_base(b, (unsigned int)p, 16, 0, 0);
}

/* ========================= CORE FORMATTER ========================= */

int vsnprintf(char *str, size_t size, const char *format, va_list args) {
    sn_buf_t buf;
    buf.buf = str;
    buf.size = size;
    buf.pos = 0;
    buf.total = 0;

    if (size > 0)
        str[0] = '\0';

    while (*format) {
        if (*format != '%') {
            sn_write_char(&buf, *format++);
            continue;
        }

        format++;

        int zero_pad = 0;
        int width = 0;

        if (*format == '0') {
            zero_pad = 1;
            format++;
        }

        while (*format >= '0' && *format <= '9') {
            width = width * 10 + (*format - '0');
            format++;
        }

        if (*format == '\0')
            break;

        int is_long = 0;
        if (*format == 'l') {
            is_long = 1;
            format++;
            if (*format == '\0')
                break;
        }

        switch (*format) {
            case 'd':
                if (is_long)
                    sn_print_long(&buf, va_arg(args, long), width, zero_pad);
                else
                    sn_print_int(&buf, va_arg(args, int), width, zero_pad);
                break;

            case 'u':
                sn_print_uint_base(&buf,
                                   va_arg(args, unsigned int),
                                   10,
                                   width,
                                   zero_pad);
                break;

            case 'x':
                sn_print_uint_base(&buf,
                                   va_arg(args, unsigned int),
                                   16,
                                   width,
                                   zero_pad);
                break;

            case 'p':
                sn_print_pointer(&buf, va_arg(args, void *));
                break;

            case 's': {
                char *s = va_arg(args, char *);
                if (!s)
                    s = "(null)";
                sn_write_str(&buf, s);
                break;
            }

            case 'c':
                sn_write_char(&buf, (char)va_arg(args, int));
                break;

            case '%':
                sn_write_char(&buf, '%');
                break;

            default:
                sn_write_char(&buf, '%');
                sn_write_char(&buf, *format);
                break;
        }

        format++;
    }

    if (size > 0) {
        if (buf.pos < size)
            str[buf.pos] = '\0';
        else
            str[size - 1] = '\0';
    }

    return buf.total;
}

/* ========================= PUBLIC API ========================= */

int snprintf(char *str, size_t size, const char *format, ...) {
    va_list args;
    va_start(args, format);
    int ret = vsnprintf(str, size, format, args);
    va_end(args);
    return ret;
}

int printf(const char *format, ...) {
    char buffer[4096];

    va_list args;
    va_start(args, format);
    int len = vsnprintf(buffer, sizeof(buffer), format, args);
    va_end(args);

    if (len > 0) {
        int write_len = (len < (int)sizeof(buffer))
                        ? len
                        : (int)sizeof(buffer);
        write(1, buffer, write_len);
    }

    return len;
}