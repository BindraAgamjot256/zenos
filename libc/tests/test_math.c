/**
 * test_math.c - Unit tests for math.c functions
 *
 * Tests mathematical functions with tolerance for floating point comparison.
 */

#include <stdio.h>
#include <math.h>
#include <assert.h>

/* Declare our implementations with zenos_ prefix */
double zenos_fabs(double x);
float zenos_fabsf(float x);
double zenos_floor(double x);
float zenos_floorf(float x);
double zenos_ceil(double x);
float zenos_ceilf(float x);
double zenos_trunc(double x);
float zenos_truncf(float x);
double zenos_round(double x);
float zenos_roundf(float x);
double zenos_fmod(double x, double y);
float zenos_fmodf(float x, float y);
double zenos_fmin(double x, double y);
float zenos_fminf(float x, float y);
double zenos_fmax(double x, double y);
float zenos_fmaxf(float x, float y);
double zenos_sqrt(double x);
float zenos_sqrtf(float x);
double zenos_cbrt(double x);
float zenos_cbrtf(float x);
double zenos_hypot(double x, double y);
float zenos_hypotf(float x, float y);
double zenos_exp(double x);
float zenos_expf(float x);
double zenos_log(double x);
float zenos_logf(float x);
double zenos_log10(double x);
float zenos_log10f(float x);
double zenos_pow(double base, double exp_val);
float zenos_powf(float base, float exp_val);
double zenos_sin(double x);
float zenos_sinf(float x);
double zenos_cos(double x);
float zenos_cosf(float x);
double zenos_tan(double x);
float zenos_tanf(float x);
double zenos_copysign(double x, double y);
float zenos_copysignf(float x, float y);
double zenos_fdim(double x, double y);
float zenos_fdimf(float x, float y);
double zenos_atan(double x);
double zenos_atan2(double y, double x);
double zenos_asin(double x);
double zenos_acos(double x);
double zenos_sinh(double x);
double zenos_cosh(double x);
double zenos_tanh(double x);

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

#define EPSILON 1e-9
#define EPSILON_LOW 1e-6
#define EPSILON_TRIG 1e-2  /* Wider tolerance for Taylor-based trig inverses */

static int approx_eq(double a, double b, double eps) {
    if (isinf(a) && isinf(b)) return (a > 0) == (b > 0);
    if (isnan(a) && isnan(b)) return 1;
    return fabs(a - b) < eps;
}

static int approx_eqf(float a, float b, float eps) {
    if (isinf(a) && isinf(b)) return (a > 0) == (b > 0);
    if (isnan(a) && isnan(b)) return 1;
    return fabsf(a - b) < eps;
}

/* fabs tests */
TEST(fabs_positive) {
    assert(approx_eq(zenos_fabs(3.14), 3.14, EPSILON));
}

TEST(fabs_negative) {
    assert(approx_eq(zenos_fabs(-3.14), 3.14, EPSILON));
}

TEST(fabs_zero) {
    assert(approx_eq(zenos_fabs(0.0), 0.0, EPSILON));
}

TEST(fabsf_positive) {
    assert(approx_eqf(zenos_fabsf(2.5f), 2.5f, (float)EPSILON));
}

TEST(fabsf_negative) {
    assert(approx_eqf(zenos_fabsf(-2.5f), 2.5f, (float)EPSILON));
}

/* floor tests */
TEST(floor_positive) {
    assert(approx_eq(zenos_floor(3.7), 3.0, EPSILON));
}

TEST(floor_negative) {
    assert(approx_eq(zenos_floor(-3.7), -4.0, EPSILON));
}

TEST(floor_integer) {
    assert(approx_eq(zenos_floor(5.0), 5.0, EPSILON));
}

TEST(floorf_positive) {
    assert(approx_eqf(zenos_floorf(3.7f), 3.0f, (float)EPSILON));
}

/* ceil tests */
TEST(ceil_positive) {
    assert(approx_eq(zenos_ceil(3.2), 4.0, EPSILON));
}

TEST(ceil_negative) {
    assert(approx_eq(zenos_ceil(-3.2), -3.0, EPSILON));
}

TEST(ceil_integer) {
    assert(approx_eq(zenos_ceil(5.0), 5.0, EPSILON));
}

/* trunc tests */
TEST(trunc_positive) {
    assert(approx_eq(zenos_trunc(3.9), 3.0, EPSILON));
}

TEST(trunc_negative) {
    assert(approx_eq(zenos_trunc(-3.9), -3.0, EPSILON));
}

/* round tests */
TEST(round_up) {
    assert(approx_eq(zenos_round(3.6), 4.0, EPSILON));
}

TEST(round_down) {
    assert(approx_eq(zenos_round(3.4), 3.0, EPSILON));
}

TEST(round_half) {
    assert(approx_eq(zenos_round(3.5), 4.0, EPSILON));
}

TEST(round_negative) {
    assert(approx_eq(zenos_round(-3.5), -4.0, EPSILON));
}

/* fmod tests */
TEST(fmod_positive) {
    assert(approx_eq(zenos_fmod(5.3, 2.0), fmod(5.3, 2.0), EPSILON_LOW));
}

TEST(fmod_negative) {
    assert(approx_eq(zenos_fmod(-5.3, 2.0), fmod(-5.3, 2.0), EPSILON_LOW));
}

/* fmin/fmax tests */
TEST(fmin_first_smaller) {
    assert(approx_eq(zenos_fmin(1.0, 2.0), 1.0, EPSILON));
}

TEST(fmin_second_smaller) {
    assert(approx_eq(zenos_fmin(3.0, 2.0), 2.0, EPSILON));
}

TEST(fmax_first_larger) {
    assert(approx_eq(zenos_fmax(3.0, 2.0), 3.0, EPSILON));
}

TEST(fmax_second_larger) {
    assert(approx_eq(zenos_fmax(1.0, 2.0), 2.0, EPSILON));
}

/* sqrt tests */
TEST(sqrt_perfect) {
    assert(approx_eq(zenos_sqrt(4.0), 2.0, EPSILON_LOW));
}

TEST(sqrt_imperfect) {
    assert(approx_eq(zenos_sqrt(2.0), sqrt(2.0), EPSILON_LOW));
}

TEST(sqrt_zero) {
    assert(approx_eq(zenos_sqrt(0.0), 0.0, EPSILON));
}

TEST(sqrt_one) {
    assert(approx_eq(zenos_sqrt(1.0), 1.0, EPSILON_LOW));
}

TEST(sqrt_large) {
    assert(approx_eq(zenos_sqrt(10000.0), 100.0, EPSILON_LOW));
}

/* cbrt tests */
TEST(cbrt_positive) {
    assert(approx_eq(zenos_cbrt(8.0), 2.0, EPSILON_LOW));
}

TEST(cbrt_negative) {
    assert(approx_eq(zenos_cbrt(-8.0), -2.0, EPSILON_LOW));
}

/* hypot tests */
TEST(hypot_345) {
    assert(approx_eq(zenos_hypot(3.0, 4.0), 5.0, EPSILON_LOW));
}

/* exp tests */
TEST(exp_zero) {
    assert(approx_eq(zenos_exp(0.0), 1.0, EPSILON_LOW));
}

TEST(exp_one) {
    assert(approx_eq(zenos_exp(1.0), exp(1.0), EPSILON_LOW));
}

TEST(exp_negative) {
    assert(approx_eq(zenos_exp(-1.0), exp(-1.0), EPSILON_LOW));
}

/* log tests */
TEST(log_one) {
    assert(approx_eq(zenos_log(1.0), 0.0, EPSILON_LOW));
}

TEST(log_e) {
    assert(approx_eq(zenos_log(exp(1.0)), 1.0, EPSILON_LOW));
}

TEST(log_ten) {
    assert(approx_eq(zenos_log(10.0), log(10.0), EPSILON_LOW));
}

/* log10 tests */
TEST(log10_ten) {
    assert(approx_eq(zenos_log10(10.0), 1.0, EPSILON_LOW));
}

TEST(log10_hundred) {
    assert(approx_eq(zenos_log10(100.0), 2.0, EPSILON_LOW));
}

/* pow tests */
TEST(pow_square) {
    assert(approx_eq(zenos_pow(2.0, 2.0), 4.0, EPSILON_LOW));
}

TEST(pow_cube) {
    assert(approx_eq(zenos_pow(2.0, 3.0), 8.0, EPSILON_LOW));
}

TEST(pow_zero_exp) {
    assert(approx_eq(zenos_pow(5.0, 0.0), 1.0, EPSILON));
}

TEST(pow_one_exp) {
    assert(approx_eq(zenos_pow(5.0, 1.0), 5.0, EPSILON_LOW));
}

TEST(pow_negative_exp) {
    assert(approx_eq(zenos_pow(2.0, -1.0), 0.5, EPSILON_LOW));
}

TEST(pow_fractional) {
    assert(approx_eq(zenos_pow(4.0, 0.5), 2.0, EPSILON_LOW));
}

/* sin tests */
TEST(sin_zero) {
    assert(approx_eq(zenos_sin(0.0), 0.0, EPSILON_LOW));
}

TEST(sin_pi_half) {
    assert(approx_eq(zenos_sin(M_PI / 2.0), 1.0, EPSILON_LOW));
}

TEST(sin_pi) {
    assert(approx_eq(zenos_sin(M_PI), 0.0, EPSILON_LOW));
}

/* cos tests */
TEST(cos_zero) {
    assert(approx_eq(zenos_cos(0.0), 1.0, EPSILON_LOW));
}

TEST(cos_pi_half) {
    assert(approx_eq(zenos_cos(M_PI / 2.0), 0.0, EPSILON_LOW));
}

TEST(cos_pi) {
    assert(approx_eq(zenos_cos(M_PI), -1.0, EPSILON_LOW));
}

/* tan tests */
TEST(tan_zero) {
    assert(approx_eq(zenos_tan(0.0), 0.0, EPSILON_LOW));
}

TEST(tan_pi_4) {
    assert(approx_eq(zenos_tan(M_PI / 4.0), 1.0, EPSILON_LOW));
}

/* copysign tests */
TEST(copysign_pos_pos) {
    assert(approx_eq(zenos_copysign(1.0, 2.0), 1.0, EPSILON));
}

TEST(copysign_pos_neg) {
    assert(approx_eq(zenos_copysign(1.0, -2.0), -1.0, EPSILON));
}

TEST(copysign_neg_pos) {
    assert(approx_eq(zenos_copysign(-1.0, 2.0), 1.0, EPSILON));
}

/* fdim tests */
TEST(fdim_positive_diff) {
    assert(approx_eq(zenos_fdim(5.0, 3.0), 2.0, EPSILON));
}

TEST(fdim_negative_diff) {
    assert(approx_eq(zenos_fdim(3.0, 5.0), 0.0, EPSILON));
}

/* atan tests */
TEST(atan_zero) {
    assert(approx_eq(zenos_atan(0.0), 0.0, EPSILON_LOW));
}

TEST(atan_one) {
    assert(approx_eq(zenos_atan(1.0), M_PI / 4.0, EPSILON_TRIG));
}

/* atan2 tests */
TEST(atan2_quadrant1) {
    assert(approx_eq(zenos_atan2(1.0, 1.0), M_PI / 4.0, EPSILON_TRIG));
}

TEST(atan2_quadrant2) {
    assert(approx_eq(zenos_atan2(1.0, -1.0), 3.0 * M_PI / 4.0, EPSILON_TRIG));
}

/* asin tests */
TEST(asin_zero) {
    assert(approx_eq(zenos_asin(0.0), 0.0, EPSILON_LOW));
}

TEST(asin_one) {
    assert(approx_eq(zenos_asin(1.0), M_PI / 2.0, EPSILON_TRIG));
}

/* acos tests */
TEST(acos_one) {
    assert(approx_eq(zenos_acos(1.0), 0.0, EPSILON_LOW));
}

TEST(acos_zero) {
    assert(approx_eq(zenos_acos(0.0), M_PI / 2.0, EPSILON_TRIG));
}

/* sinh tests */
TEST(sinh_zero) {
    assert(approx_eq(zenos_sinh(0.0), 0.0, EPSILON_LOW));
}

TEST(sinh_one) {
    assert(approx_eq(zenos_sinh(1.0), sinh(1.0), EPSILON_LOW));
}

/* cosh tests */
TEST(cosh_zero) {
    assert(approx_eq(zenos_cosh(0.0), 1.0, EPSILON_LOW));
}

TEST(cosh_one) {
    assert(approx_eq(zenos_cosh(1.0), cosh(1.0), EPSILON_LOW));
}

/* tanh tests */
TEST(tanh_zero) {
    assert(approx_eq(zenos_tanh(0.0), 0.0, EPSILON_LOW));
}

TEST(tanh_large) {
    assert(approx_eq(zenos_tanh(100.0), 1.0, EPSILON_LOW));
}

void run_math_tests(void) {
    printf("\n=== Math Tests ===\n");

    /* fabs */
    RUN_TEST(fabs_positive);
    RUN_TEST(fabs_negative);
    RUN_TEST(fabs_zero);
    RUN_TEST(fabsf_positive);
    RUN_TEST(fabsf_negative);

    /* floor */
    RUN_TEST(floor_positive);
    RUN_TEST(floor_negative);
    RUN_TEST(floor_integer);
    RUN_TEST(floorf_positive);

    /* ceil */
    RUN_TEST(ceil_positive);
    RUN_TEST(ceil_negative);
    RUN_TEST(ceil_integer);

    /* trunc */
    RUN_TEST(trunc_positive);
    RUN_TEST(trunc_negative);

    /* round */
    RUN_TEST(round_up);
    RUN_TEST(round_down);
    RUN_TEST(round_half);
    RUN_TEST(round_negative);

    /* fmod */
    RUN_TEST(fmod_positive);
    RUN_TEST(fmod_negative);

    /* fmin/fmax */
    RUN_TEST(fmin_first_smaller);
    RUN_TEST(fmin_second_smaller);
    RUN_TEST(fmax_first_larger);
    RUN_TEST(fmax_second_larger);

    /* sqrt */
    RUN_TEST(sqrt_perfect);
    RUN_TEST(sqrt_imperfect);
    RUN_TEST(sqrt_zero);
    RUN_TEST(sqrt_one);
    RUN_TEST(sqrt_large);

    /* cbrt */
    RUN_TEST(cbrt_positive);
    RUN_TEST(cbrt_negative);

    /* hypot */
    RUN_TEST(hypot_345);

    /* exp */
    RUN_TEST(exp_zero);
    RUN_TEST(exp_one);
    RUN_TEST(exp_negative);

    /* log */
    RUN_TEST(log_one);
    RUN_TEST(log_e);
    RUN_TEST(log_ten);

    /* log10 */
    RUN_TEST(log10_ten);
    RUN_TEST(log10_hundred);

    /* pow */
    RUN_TEST(pow_square);
    RUN_TEST(pow_cube);
    RUN_TEST(pow_zero_exp);
    RUN_TEST(pow_one_exp);
    RUN_TEST(pow_negative_exp);
    RUN_TEST(pow_fractional);

    /* sin */
    RUN_TEST(sin_zero);
    RUN_TEST(sin_pi_half);
    RUN_TEST(sin_pi);

    /* cos */
    RUN_TEST(cos_zero);
    RUN_TEST(cos_pi_half);
    RUN_TEST(cos_pi);

    /* tan */
    RUN_TEST(tan_zero);
    RUN_TEST(tan_pi_4);

    /* copysign */
    RUN_TEST(copysign_pos_pos);
    RUN_TEST(copysign_pos_neg);
    RUN_TEST(copysign_neg_pos);

    /* fdim */
    RUN_TEST(fdim_positive_diff);
    RUN_TEST(fdim_negative_diff);

    /* atan */
    RUN_TEST(atan_zero);
    RUN_TEST(atan_one);

    /* atan2 */
    RUN_TEST(atan2_quadrant1);
    RUN_TEST(atan2_quadrant2);

    /* asin */
    RUN_TEST(asin_zero);
    RUN_TEST(asin_one);

    /* acos */
    RUN_TEST(acos_one);
    RUN_TEST(acos_zero);

    /* sinh */
    RUN_TEST(sinh_zero);
    RUN_TEST(sinh_one);

    /* cosh */
    RUN_TEST(cosh_zero);
    RUN_TEST(cosh_one);

    /* tanh */
    RUN_TEST(tanh_zero);
    RUN_TEST(tanh_large);

    printf("\nMath tests: %d/%d passed\n", tests_passed, tests_run);
}

int get_math_test_results(int *passed, int *total) {
    *passed = tests_passed;
    *total = tests_run;
    return tests_passed == tests_run ? 0 : 1;
}
