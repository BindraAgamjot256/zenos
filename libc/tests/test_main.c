/**
 * test_main.c - Test runner for libc unit tests
 *
 * Runs all test suites and reports results.
 */

#include <stdio.h>

/* Test suite runners */
void run_string_tests(void);
void run_stdlib_tests(void);
void run_math_tests(void);

/* Result getters */
int get_string_test_results(int *passed, int *total);
int get_stdlib_test_results(int *passed, int *total);
int get_math_test_results(int *passed, int *total);

int main(void) {
    int total_passed = 0;
    int total_tests = 0;
    int passed, total;
    int failures = 0;

    printf("\n");
    printf("========================================\n");
    printf("  Zenos libc Unit Tests (Host)\n");
    printf("========================================\n");

    /* Run all test suites */
    run_string_tests();
    failures += get_string_test_results(&passed, &total);
    total_passed += passed;
    total_tests += total;

    run_stdlib_tests();
    failures += get_stdlib_test_results(&passed, &total);
    total_passed += passed;
    total_tests += total;

    run_math_tests();
    failures += get_math_test_results(&passed, &total);
    total_passed += passed;
    total_tests += total;

    /* Summary */
    printf("\n========================================\n");
    if (failures == 0) {
        printf("  \x1b[32mAll tests passed: %d/%d\x1b[0m\n", total_passed, total_tests);
    } else {
        printf("  \x1b[31mSome tests failed: %d/%d passed\x1b[0m\n", total_passed, total_tests);
    }
    printf("========================================\n\n");

    return failures;
}
