/**
 * test_stdlib.c - Unit tests for stdlib.c functions
 *
 * Tests atoi, atol and other utility functions.
 */

#include <stdio.h>
#include <stdlib.h>
#include <assert.h>

/* Declare our implementations with zenos_ prefix */
int zenos_atoi(const char *str);
long zenos_atol(const char *str);

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

/* atoi tests */
TEST(atoi_positive) {
    assert(zenos_atoi("123") == 123);
}

TEST(atoi_negative) {
    assert(zenos_atoi("-456") == -456);
}

TEST(atoi_with_plus) {
    assert(zenos_atoi("+789") == 789);
}

TEST(atoi_with_spaces) {
    assert(zenos_atoi("  42") == 42);
}

TEST(atoi_with_tabs) {
    assert(zenos_atoi("\t\n123") == 123);
}

TEST(atoi_zero) {
    assert(zenos_atoi("0") == 0);
}

TEST(atoi_trailing_chars) {
    assert(zenos_atoi("123abc") == 123);
}

TEST(atoi_only_spaces) {
    assert(zenos_atoi("   ") == 0);
}

TEST(atoi_empty) {
    assert(zenos_atoi("") == 0);
}

TEST(atoi_large_number) {
    assert(zenos_atoi("2147483647") == 2147483647);
}

TEST(atoi_negative_large) {
    assert(zenos_atoi("-2147483648") == -2147483648);
}

/* atol tests */
TEST(atol_positive) {
    assert(zenos_atol("123456789") == 123456789L);
}

TEST(atol_negative) {
    assert(zenos_atol("-987654321") == -987654321L);
}

TEST(atol_with_spaces) {
    assert(zenos_atol("  12345") == 12345L);
}

TEST(atol_zero) {
    assert(zenos_atol("0") == 0L);
}

TEST(atol_large) {
    assert(zenos_atol("9223372036854775807") == 9223372036854775807L);
}

void run_stdlib_tests(void) {
    printf("\n=== Stdlib Tests ===\n");

    /* atoi */
    RUN_TEST(atoi_positive);
    RUN_TEST(atoi_negative);
    RUN_TEST(atoi_with_plus);
    RUN_TEST(atoi_with_spaces);
    RUN_TEST(atoi_with_tabs);
    RUN_TEST(atoi_zero);
    RUN_TEST(atoi_trailing_chars);
    RUN_TEST(atoi_only_spaces);
    RUN_TEST(atoi_empty);
    RUN_TEST(atoi_large_number);
    RUN_TEST(atoi_negative_large);

    /* atol */
    RUN_TEST(atol_positive);
    RUN_TEST(atol_negative);
    RUN_TEST(atol_with_spaces);
    RUN_TEST(atol_zero);
    RUN_TEST(atol_large);

    printf("\nStdlib tests: %d/%d passed\n", tests_passed, tests_run);
}

int get_stdlib_test_results(int *passed, int *total) {
    *passed = tests_passed;
    *total = tests_run;
    return tests_passed == tests_run ? 0 : 1;
}
