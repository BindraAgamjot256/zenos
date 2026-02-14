/**
 * test_string.c - Unit tests for string.c functions
 *
 * Tests string and memory manipulation functions against host libc.
 */

#include <stdio.h>
#include <string.h>
#include <assert.h>

/* Declare our implementations with zenos_ prefix */
size_t zenos_strlen(const char *s);
char *zenos_strcpy(char *dest, const char *src);
char *zenos_strncpy(char *dest, const char *src, size_t n);
int zenos_strcmp(const char *s1, const char *s2);
int zenos_strncmp(const char *s1, const char *s2, size_t n);
char *zenos_strcat(char *dest, const char *src);
char *zenos_strncat(char *dest, const char *src, size_t n);
char *zenos_strchr(const char *s, int c);
char *zenos_strrchr(const char *s, int c);
char *zenos_strstr(const char *haystack, const char *needle);
void *zenos_memset(void *s, int c, size_t n);
int zenos_memcmp(const void *s1, const void *s2, size_t n);
void *zenos_memcpy(void *dest, const void *src, size_t n);
void *zenos_memmove(void *dest, const void *src, size_t n);

static int tests_run = 0;
static int tests_passed = 0;

#define TEST(name) static void test_##name(void)
#define RUN_TEST(name) do { \
    printf("  %-40s ", #name); \
    tests_run++; \
    test_##name(); \
    tests_passed++; \
    printf("\x1b[32mPASSED\x1b[0m\n"); \
} while(0)

/* strlen tests */
TEST(strlen_empty) {
    assert(zenos_strlen("") == 0);
}

TEST(strlen_simple) {
    assert(zenos_strlen("hello") == 5);
}

TEST(strlen_with_spaces) {
    assert(zenos_strlen("hello world") == 11);
}

TEST(strlen_single_char) {
    assert(zenos_strlen("x") == 1);
}

/* strcpy tests */
TEST(strcpy_simple) {
    char dest[32];
    zenos_strcpy(dest, "hello");
    assert(strcmp(dest, "hello") == 0);
}

TEST(strcpy_empty) {
    char dest[32] = "garbage";
    zenos_strcpy(dest, "");
    assert(strcmp(dest, "") == 0);
}

TEST(strcpy_returns_dest) {
    char dest[32];
    assert(zenos_strcpy(dest, "test") == dest);
}

/* strncpy tests */
TEST(strncpy_exact) {
    char dest[6];
    zenos_strncpy(dest, "hello", 6);
    assert(strcmp(dest, "hello") == 0);
}

TEST(strncpy_truncate) {
    char dest[4];
    zenos_strncpy(dest, "hello", 3);
    dest[3] = '\0';
    assert(strncmp(dest, "hel", 3) == 0);
}

TEST(strncpy_pad_zeros) {
    char dest[10] = "xxxxxxxxx";
    zenos_strncpy(dest, "hi", 10);
    assert(dest[2] == '\0');
    assert(dest[9] == '\0');
}

/* strcmp tests */
TEST(strcmp_equal) {
    assert(zenos_strcmp("hello", "hello") == 0);
}

TEST(strcmp_less) {
    assert(zenos_strcmp("abc", "abd") < 0);
}

TEST(strcmp_greater) {
    assert(zenos_strcmp("abd", "abc") > 0);
}

TEST(strcmp_empty) {
    assert(zenos_strcmp("", "") == 0);
}

TEST(strcmp_prefix) {
    assert(zenos_strcmp("hello", "helloworld") < 0);
}

/* strncmp tests */
TEST(strncmp_equal_within_n) {
    assert(zenos_strncmp("hello", "helps", 3) == 0);
}

TEST(strncmp_differ_within_n) {
    assert(zenos_strncmp("hello", "hallo", 3) != 0);
}

TEST(strncmp_zero_n) {
    assert(zenos_strncmp("abc", "xyz", 0) == 0);
}

/* strcat tests */
TEST(strcat_simple) {
    char dest[32] = "hello";
    zenos_strcat(dest, " world");
    assert(strcmp(dest, "hello world") == 0);
}

TEST(strcat_empty_src) {
    char dest[32] = "hello";
    zenos_strcat(dest, "");
    assert(strcmp(dest, "hello") == 0);
}

TEST(strcat_empty_dest) {
    char dest[32] = "";
    zenos_strcat(dest, "hello");
    assert(strcmp(dest, "hello") == 0);
}

/* strncat tests */
TEST(strncat_partial) {
    char dest[32] = "hello";
    zenos_strncat(dest, " world", 3);
    assert(strcmp(dest, "hello wo") == 0);
}

TEST(strncat_full) {
    char dest[32] = "hello";
    zenos_strncat(dest, " world", 10);
    assert(strcmp(dest, "hello world") == 0);
}

/* strchr tests */
TEST(strchr_found) {
    const char *s = "hello";
    assert(zenos_strchr(s, 'l') == s + 2);
}

TEST(strchr_not_found) {
    assert(zenos_strchr("hello", 'x') == NULL);
}

TEST(strchr_null_terminator) {
    const char *s = "hello";
    assert(zenos_strchr(s, '\0') == s + 5);
}

TEST(strchr_first_char) {
    const char *s = "hello";
    assert(zenos_strchr(s, 'h') == s);
}

/* strrchr tests */
TEST(strrchr_found_last) {
    const char *s = "hello";
    assert(zenos_strrchr(s, 'l') == s + 3);
}

TEST(strrchr_not_found) {
    assert(zenos_strrchr("hello", 'x') == NULL);
}

TEST(strrchr_null_terminator) {
    const char *s = "hello";
    assert(zenos_strrchr(s, '\0') == s + 5);
}

/* strstr tests */
TEST(strstr_found) {
    const char *s = "hello world";
    assert(zenos_strstr(s, "world") == s + 6);
}

TEST(strstr_not_found) {
    assert(zenos_strstr("hello", "xyz") == NULL);
}

TEST(strstr_empty_needle) {
    const char *s = "hello";
    assert(zenos_strstr(s, "") == s);
}

TEST(strstr_at_start) {
    const char *s = "hello world";
    assert(zenos_strstr(s, "hello") == s);
}

TEST(strstr_same) {
    const char *s = "hello";
    assert(zenos_strstr(s, "hello") == s);
}

/* memset tests */
TEST(memset_zero) {
    char buf[10] = "xxxxxxxxx";
    zenos_memset(buf, 0, 5);
    assert(buf[0] == 0 && buf[4] == 0);
    assert(buf[5] == 'x');
}

TEST(memset_char) {
    char buf[10];
    zenos_memset(buf, 'A', 10);
    for (int i = 0; i < 10; i++) {
        assert(buf[i] == 'A');
    }
}

TEST(memset_returns_ptr) {
    char buf[10];
    assert(zenos_memset(buf, 0, 10) == buf);
}

/* memcmp tests */
TEST(memcmp_equal) {
    assert(zenos_memcmp("hello", "hello", 5) == 0);
}

TEST(memcmp_less) {
    assert(zenos_memcmp("abc", "abd", 3) < 0);
}

TEST(memcmp_greater) {
    assert(zenos_memcmp("abd", "abc", 3) > 0);
}

TEST(memcmp_partial) {
    assert(zenos_memcmp("hello", "helps", 3) == 0);
}

/* memcpy tests */
TEST(memcpy_simple) {
    char src[] = "hello";
    char dest[10];
    zenos_memcpy(dest, src, 6);
    assert(strcmp(dest, "hello") == 0);
}

TEST(memcpy_returns_dest) {
    char src[] = "test";
    char dest[10];
    assert(zenos_memcpy(dest, src, 5) == dest);
}

/* memmove tests */
TEST(memmove_non_overlapping) {
    char src[] = "hello";
    char dest[10];
    zenos_memmove(dest, src, 6);
    assert(strcmp(dest, "hello") == 0);
}

TEST(memmove_overlap_forward) {
    char buf[] = "hello world";
    zenos_memmove(buf + 2, buf, 5);
    assert(strncmp(buf + 2, "hello", 5) == 0);
}

TEST(memmove_overlap_backward) {
    char buf[] = "hello world";
    zenos_memmove(buf, buf + 6, 5);
    assert(strncmp(buf, "world", 5) == 0);
}

void run_string_tests(void) {
    printf("\n=== String Tests ===\n");

    /* strlen */
    RUN_TEST(strlen_empty);
    RUN_TEST(strlen_simple);
    RUN_TEST(strlen_with_spaces);
    RUN_TEST(strlen_single_char);

    /* strcpy */
    RUN_TEST(strcpy_simple);
    RUN_TEST(strcpy_empty);
    RUN_TEST(strcpy_returns_dest);

    /* strncpy */
    RUN_TEST(strncpy_exact);
    RUN_TEST(strncpy_truncate);
    RUN_TEST(strncpy_pad_zeros);

    /* strcmp */
    RUN_TEST(strcmp_equal);
    RUN_TEST(strcmp_less);
    RUN_TEST(strcmp_greater);
    RUN_TEST(strcmp_empty);
    RUN_TEST(strcmp_prefix);

    /* strncmp */
    RUN_TEST(strncmp_equal_within_n);
    RUN_TEST(strncmp_differ_within_n);
    RUN_TEST(strncmp_zero_n);

    /* strcat */
    RUN_TEST(strcat_simple);
    RUN_TEST(strcat_empty_src);
    RUN_TEST(strcat_empty_dest);

    /* strncat */
    RUN_TEST(strncat_partial);
    RUN_TEST(strncat_full);

    /* strchr */
    RUN_TEST(strchr_found);
    RUN_TEST(strchr_not_found);
    RUN_TEST(strchr_null_terminator);
    RUN_TEST(strchr_first_char);

    /* strrchr */
    RUN_TEST(strrchr_found_last);
    RUN_TEST(strrchr_not_found);
    RUN_TEST(strrchr_null_terminator);

    /* strstr */
    RUN_TEST(strstr_found);
    RUN_TEST(strstr_not_found);
    RUN_TEST(strstr_empty_needle);
    RUN_TEST(strstr_at_start);
    RUN_TEST(strstr_same);

    /* memset */
    RUN_TEST(memset_zero);
    RUN_TEST(memset_char);
    RUN_TEST(memset_returns_ptr);

    /* memcmp */
    RUN_TEST(memcmp_equal);
    RUN_TEST(memcmp_less);
    RUN_TEST(memcmp_greater);
    RUN_TEST(memcmp_partial);

    /* memcpy */
    RUN_TEST(memcpy_simple);
    RUN_TEST(memcpy_returns_dest);

    /* memmove */
    RUN_TEST(memmove_non_overlapping);
    RUN_TEST(memmove_overlap_forward);
    RUN_TEST(memmove_overlap_backward);

    printf("\nString tests: %d/%d passed\n", tests_passed, tests_run);
}

int get_string_test_results(int *passed, int *total) {
    *passed = tests_passed;
    *total = tests_run;
    return tests_passed == tests_run ? 0 : 1;
}
