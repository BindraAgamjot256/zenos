#include <criterion/criterion.h>
#include <math.h>
#include <stddef.h>

#define DECLARE_UNARY(name)                                                    \
  double zenos_##name(double value);                                           \
  float zenos_##name##f(float value)
#define DECLARE_BINARY(name)                                                   \
  double zenos_##name(double left, double right);                              \
  float zenos_##name##f(float left, float right)

DECLARE_UNARY(fabs);
DECLARE_UNARY(floor);
DECLARE_UNARY(ceil);
DECLARE_UNARY(trunc);
DECLARE_UNARY(round);
DECLARE_UNARY(sqrt);
DECLARE_UNARY(cbrt);
DECLARE_UNARY(exp);
DECLARE_UNARY(exp2);
DECLARE_UNARY(expm1);
DECLARE_UNARY(log);
DECLARE_UNARY(log2);
DECLARE_UNARY(log10);
DECLARE_UNARY(log1p);
DECLARE_UNARY(sin);
DECLARE_UNARY(cos);
DECLARE_UNARY(tan);
DECLARE_UNARY(atan);
DECLARE_UNARY(asin);
DECLARE_UNARY(acos);
DECLARE_UNARY(sinh);
DECLARE_UNARY(cosh);
DECLARE_UNARY(tanh);
DECLARE_UNARY(asinh);
DECLARE_UNARY(acosh);
DECLARE_UNARY(atanh);
DECLARE_UNARY(logb);

DECLARE_BINARY(copysign);
DECLARE_BINARY(fmod);
DECLARE_BINARY(fmin);
DECLARE_BINARY(fmax);
DECLARE_BINARY(fdim);
DECLARE_BINARY(hypot);
DECLARE_BINARY(pow);
DECLARE_BINARY(atan2);
DECLARE_BINARY(nextafter);
DECLARE_BINARY(remainder);

double zenos_modf(double value, double *integer_part);
float zenos_modff(float value, float *integer_part);
double zenos_ldexp(double value, int exponent);
float zenos_ldexpf(float value, int exponent);
double zenos_frexp(double value, int *exponent);
float zenos_frexpf(float value, int *exponent);
double zenos_scalbn(double value, int exponent);
float zenos_scalbnf(float value, int exponent);
int zenos_ilogb(double value);
int zenos_ilogbf(float value);
double zenos_fma(double x, double y, double z);
float zenos_fmaf(float x, float y, float z);

#define ARRAY_LEN(array) (sizeof(array) / sizeof((array)[0]))
#define DOUBLE_TOLERANCE 1e-8
#define FLOAT_TOLERANCE 1e-5f
#define INVERSE_TRIG_TOLERANCE 1e-2
#define INVERSE_TRIG_FLOAT_TOLERANCE 1e-2f

static void expect_double(const char *function, size_t case_index,
                          double actual, double expected, double tolerance) {
  if (isnan(expected)) {
    cr_expect(isnan(actual), "%s case %zu: expected NaN, got %.17g", function,
              case_index, actual);
    return;
  }
  if (isinf(expected)) {
    cr_expect(isinf(actual) && !!signbit(actual) == !!signbit(expected),
              "%s case %zu: expected %.17g, got %.17g", function, case_index,
              expected, actual);
    return;
  }
  if (expected == 0.0 && actual == 0.0) {
    cr_expect_eq(!!signbit(actual), !!signbit(expected),
                 "%s case %zu: zero signs differed", function, case_index);
    return;
  }

  double scale = fmax(1.0, fabs(expected));
  cr_expect_leq(fabs(actual - expected), tolerance * scale,
                "%s case %zu: expected %.17g, got %.17g", function, case_index,
                expected, actual);
}

static void expect_float(const char *function, size_t case_index, float actual,
                         float expected, float tolerance) {
  if (isnan(expected)) {
    cr_expect(isnan(actual), "%s case %zu: expected NaN, got %.9g", function,
              case_index, (double)actual);
    return;
  }
  if (isinf(expected)) {
    cr_expect(isinf(actual) && !!signbit(actual) == !!signbit(expected),
              "%s case %zu: expected %.9g, got %.9g", function, case_index,
              (double)expected, (double)actual);
    return;
  }
  if (expected == 0.0f && actual == 0.0f) {
    cr_expect_eq(!!signbit(actual), !!signbit(expected),
                 "%s case %zu: zero signs differed", function, case_index);
    return;
  }

  float scale = fmaxf(1.0f, fabsf(expected));
  cr_expect_leq(fabsf(actual - expected), tolerance * scale,
                "%s case %zu: expected %.9g, got %.9g", function, case_index,
                (double)expected, (double)actual);
}

#define DEFINE_UNARY_DOUBLE_TEST(name, tolerance, ...)                         \
  Test(math, name##_matches_host) {                                            \
    const double inputs[] = {__VA_ARGS__};                                     \
    for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {                           \
      expect_double(#name, i, zenos_##name(inputs[i]), name(inputs[i]),        \
                    tolerance);                                                \
    }                                                                          \
  }

#define DEFINE_UNARY_FLOAT_TEST(name, tolerance, ...)                          \
  Test(math, name##f_matches_host) {                                           \
    const float inputs[] = {__VA_ARGS__};                                      \
    for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {                           \
      expect_float(#name "f", i, zenos_##name##f(inputs[i]),                   \
                   name##f(inputs[i]), tolerance);                             \
    }                                                                          \
  }

#define DEFINE_BINARY_DOUBLE_TEST(name, tolerance, ...)                        \
  Test(math, name##_matches_host) {                                            \
    const struct {                                                             \
      double left;                                                             \
      double right;                                                            \
    } inputs[] = {__VA_ARGS__};                                                \
    for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {                           \
      expect_double(#name, i, zenos_##name(inputs[i].left, inputs[i].right),   \
                    name(inputs[i].left, inputs[i].right), tolerance);         \
    }                                                                          \
  }

#define DEFINE_BINARY_FLOAT_TEST(name, tolerance, ...)                         \
  Test(math, name##f_matches_host) {                                           \
    const struct {                                                             \
      float left;                                                              \
      float right;                                                             \
    } inputs[] = {__VA_ARGS__};                                                \
    for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {                           \
      expect_float(#name "f", i,                                               \
                   zenos_##name##f(inputs[i].left, inputs[i].right),           \
                   name##f(inputs[i].left, inputs[i].right), tolerance);       \
    }                                                                          \
  }

DEFINE_UNARY_DOUBLE_TEST(fabs, 0.0, -3.14, -1.0, 0.0, 2.5);
DEFINE_UNARY_FLOAT_TEST(fabs, 0.0f, -3.14f, -1.0f, 0.0f, 2.5f);
DEFINE_UNARY_DOUBLE_TEST(floor, 0.0, -3.7, -1.0, 0.0, 3.7, 5.0);
DEFINE_UNARY_FLOAT_TEST(floor, 0.0f, -3.7f, -1.0f, 0.0f, 3.7f, 5.0f);
DEFINE_UNARY_DOUBLE_TEST(ceil, 0.0, -3.7, -1.0, 0.0, 3.7, 5.0);
DEFINE_UNARY_FLOAT_TEST(ceil, 0.0f, -3.7f, -1.0f, 0.0f, 3.7f, 5.0f);
DEFINE_UNARY_DOUBLE_TEST(trunc, 0.0, -3.9, -1.0, 0.0, 3.9, 5.0);
DEFINE_UNARY_FLOAT_TEST(trunc, 0.0f, -3.9f, -1.0f, 0.0f, 3.9f, 5.0f);
DEFINE_UNARY_DOUBLE_TEST(round, 0.0, -3.5, -3.4, 0.0, 3.4, 3.5);
DEFINE_UNARY_FLOAT_TEST(round, 0.0f, -3.5f, -3.4f, 0.0f, 3.4f, 3.5f);
DEFINE_UNARY_DOUBLE_TEST(sqrt, DOUBLE_TOLERANCE, 0.0, 1.0, 2.0, 10000.0);
DEFINE_UNARY_FLOAT_TEST(sqrt, FLOAT_TOLERANCE, 0.0f, 1.0f, 2.0f, 10000.0f);
DEFINE_UNARY_DOUBLE_TEST(cbrt, DOUBLE_TOLERANCE, -27.0, -8.0, 0.0, 8.0, 27.0);
DEFINE_UNARY_FLOAT_TEST(cbrt, FLOAT_TOLERANCE, -27.0f, -8.0f, 0.0f, 8.0f,
                        27.0f);
DEFINE_UNARY_DOUBLE_TEST(exp, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 5.0);
DEFINE_UNARY_FLOAT_TEST(exp, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 5.0f);
DEFINE_UNARY_DOUBLE_TEST(exp2, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 5.0);
DEFINE_UNARY_FLOAT_TEST(exp2, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 5.0f);
DEFINE_UNARY_DOUBLE_TEST(expm1, DOUBLE_TOLERANCE, -1.0, -1e-6, 0.0, 1e-6, 1.0);
DEFINE_UNARY_FLOAT_TEST(expm1, FLOAT_TOLERANCE, -1.0f, -1e-4f, 0.0f, 1e-4f,
                        1.0f);
DEFINE_UNARY_DOUBLE_TEST(log, DOUBLE_TOLERANCE, 0.125, 0.5, 1.0, 2.0, 10.0);
DEFINE_UNARY_FLOAT_TEST(log, FLOAT_TOLERANCE, 0.125f, 0.5f, 1.0f, 2.0f, 10.0f);
DEFINE_UNARY_DOUBLE_TEST(log2, DOUBLE_TOLERANCE, 0.125, 0.5, 1.0, 2.0, 16.0);
DEFINE_UNARY_FLOAT_TEST(log2, FLOAT_TOLERANCE, 0.125f, 0.5f, 1.0f, 2.0f, 16.0f);
DEFINE_UNARY_DOUBLE_TEST(log10, DOUBLE_TOLERANCE, 0.1, 1.0, 10.0, 100.0);
DEFINE_UNARY_FLOAT_TEST(log10, FLOAT_TOLERANCE, 0.1f, 1.0f, 10.0f, 100.0f);
DEFINE_UNARY_DOUBLE_TEST(log1p, DOUBLE_TOLERANCE, -0.5, -1e-6, 0.0, 1e-6, 1.0);
DEFINE_UNARY_FLOAT_TEST(log1p, FLOAT_TOLERANCE, -0.5f, -1e-4f, 0.0f, 1e-4f,
                        1.0f);
DEFINE_UNARY_DOUBLE_TEST(sin, DOUBLE_TOLERANCE, -3.141592653589793, -1.0, 0.0,
                         1.0, 3.141592653589793);
DEFINE_UNARY_FLOAT_TEST(sin, FLOAT_TOLERANCE, -3.1415927f, -1.0f, 0.0f, 1.0f,
                        3.1415927f);
DEFINE_UNARY_DOUBLE_TEST(cos, DOUBLE_TOLERANCE, -3.141592653589793, -1.0, 0.0,
                         1.0, 3.141592653589793);
DEFINE_UNARY_FLOAT_TEST(cos, FLOAT_TOLERANCE, -3.1415927f, -1.0f, 0.0f, 1.0f,
                        3.1415927f);
DEFINE_UNARY_DOUBLE_TEST(tan, DOUBLE_TOLERANCE, -1.0, -0.5, 0.0, 0.5, 1.0);
DEFINE_UNARY_FLOAT_TEST(tan, FLOAT_TOLERANCE, -1.0f, -0.5f, 0.0f, 0.5f, 1.0f);
DEFINE_UNARY_DOUBLE_TEST(atan, INVERSE_TRIG_TOLERANCE, -2.0, -1.0, 0.0, 1.0,
                         2.0);
DEFINE_UNARY_FLOAT_TEST(atan, INVERSE_TRIG_FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f,
                        1.0f, 2.0f);
DEFINE_UNARY_DOUBLE_TEST(asin, INVERSE_TRIG_TOLERANCE, -1.0, -0.5, 0.0, 0.5,
                         1.0);
DEFINE_UNARY_FLOAT_TEST(asin, INVERSE_TRIG_FLOAT_TOLERANCE, -1.0f, -0.5f, 0.0f,
                        0.5f, 1.0f);
DEFINE_UNARY_DOUBLE_TEST(acos, INVERSE_TRIG_TOLERANCE, -1.0, -0.5, 0.0, 0.5,
                         1.0);
DEFINE_UNARY_FLOAT_TEST(acos, INVERSE_TRIG_FLOAT_TOLERANCE, -1.0f, -0.5f, 0.0f,
                        0.5f, 1.0f);
DEFINE_UNARY_DOUBLE_TEST(sinh, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 2.0);
DEFINE_UNARY_FLOAT_TEST(sinh, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 2.0f);
DEFINE_UNARY_DOUBLE_TEST(cosh, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 2.0);
DEFINE_UNARY_FLOAT_TEST(cosh, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 2.0f);
DEFINE_UNARY_DOUBLE_TEST(tanh, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 2.0);
DEFINE_UNARY_FLOAT_TEST(tanh, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 2.0f);
DEFINE_UNARY_DOUBLE_TEST(asinh, DOUBLE_TOLERANCE, -2.0, -1.0, 0.0, 1.0, 2.0);
DEFINE_UNARY_FLOAT_TEST(asinh, FLOAT_TOLERANCE, -2.0f, -1.0f, 0.0f, 1.0f, 2.0f);
DEFINE_UNARY_DOUBLE_TEST(acosh, DOUBLE_TOLERANCE, 1.0, 1.5, 2.0, 10.0);
DEFINE_UNARY_FLOAT_TEST(acosh, FLOAT_TOLERANCE, 1.0f, 1.5f, 2.0f, 10.0f);
DEFINE_UNARY_DOUBLE_TEST(atanh, DOUBLE_TOLERANCE, -0.75, -0.25, 0.0, 0.25,
                         0.75);
DEFINE_UNARY_FLOAT_TEST(atanh, FLOAT_TOLERANCE, -0.75f, -0.25f, 0.0f, 0.25f,
                        0.75f);
DEFINE_UNARY_DOUBLE_TEST(logb, 0.0, 0.125, 0.5, 1.0, 2.0, 16.0);
DEFINE_UNARY_FLOAT_TEST(logb, 0.0f, 0.125f, 0.5f, 1.0f, 2.0f, 16.0f);

DEFINE_BINARY_DOUBLE_TEST(copysign, 0.0, {1.0, 2.0}, {1.0, -2.0}, {-1.0, 2.0});
DEFINE_BINARY_FLOAT_TEST(copysign, 0.0f, {1.0f, 2.0f}, {1.0f, -2.0f},
                         {-1.0f, 2.0f});
DEFINE_BINARY_DOUBLE_TEST(fmod, DOUBLE_TOLERANCE, {5.3, 2.0}, {-5.3, 2.0},
                          {5.3, -2.0});
DEFINE_BINARY_FLOAT_TEST(fmod, FLOAT_TOLERANCE, {5.3f, 2.0f}, {-5.3f, 2.0f},
                         {5.3f, -2.0f});
DEFINE_BINARY_DOUBLE_TEST(fmin, 0.0, {1.0, 2.0}, {3.0, 2.0}, {-1.0, -2.0});
DEFINE_BINARY_FLOAT_TEST(fmin, 0.0f, {1.0f, 2.0f}, {3.0f, 2.0f},
                         {-1.0f, -2.0f});
DEFINE_BINARY_DOUBLE_TEST(fmax, 0.0, {1.0, 2.0}, {3.0, 2.0}, {-1.0, -2.0});
DEFINE_BINARY_FLOAT_TEST(fmax, 0.0f, {1.0f, 2.0f}, {3.0f, 2.0f},
                         {-1.0f, -2.0f});
DEFINE_BINARY_DOUBLE_TEST(fdim, DOUBLE_TOLERANCE, {5.0, 3.0}, {3.0, 5.0},
                          {-1.0, -2.0});
DEFINE_BINARY_FLOAT_TEST(fdim, FLOAT_TOLERANCE, {5.0f, 3.0f}, {3.0f, 5.0f},
                         {-1.0f, -2.0f});
DEFINE_BINARY_DOUBLE_TEST(hypot, DOUBLE_TOLERANCE, {3.0, 4.0}, {5.0, 12.0},
                          {-3.0, 4.0});
DEFINE_BINARY_FLOAT_TEST(hypot, FLOAT_TOLERANCE, {3.0f, 4.0f}, {5.0f, 12.0f},
                         {-3.0f, 4.0f});
DEFINE_BINARY_DOUBLE_TEST(pow, DOUBLE_TOLERANCE, {2.0, 3.0}, {2.0, -1.0},
                          {4.0, 0.5}, {5.0, 0.0});
DEFINE_BINARY_FLOAT_TEST(pow, FLOAT_TOLERANCE, {2.0f, 3.0f}, {2.0f, -1.0f},
                         {4.0f, 0.5f}, {5.0f, 0.0f});
DEFINE_BINARY_DOUBLE_TEST(atan2, INVERSE_TRIG_TOLERANCE, {1.0, 1.0},
                          {1.0, -1.0}, {-1.0, -1.0}, {-1.0, 1.0});
DEFINE_BINARY_FLOAT_TEST(atan2, INVERSE_TRIG_FLOAT_TOLERANCE, {1.0f, 1.0f},
                         {1.0f, -1.0f}, {-1.0f, -1.0f}, {-1.0f, 1.0f});
DEFINE_BINARY_DOUBLE_TEST(nextafter, 0.0, {0.0, 1.0}, {1.0, 2.0}, {1.0, 0.0},
                          {-1.0, -2.0});
DEFINE_BINARY_FLOAT_TEST(nextafter, 0.0f, {0.0f, 1.0f}, {1.0f, 2.0f},
                         {1.0f, 0.0f}, {-1.0f, -2.0f});
DEFINE_BINARY_DOUBLE_TEST(remainder, DOUBLE_TOLERANCE, {5.3, 2.0}, {-5.3, 2.0},
                          {6.0, 4.0});
DEFINE_BINARY_FLOAT_TEST(remainder, FLOAT_TOLERANCE, {5.3f, 2.0f},
                         {-5.3f, 2.0f}, {6.0f, 4.0f});

Test(math, modf_matches_host) {
  const double inputs[] = {-3.75, -1.0, 0.0, 1.0, 3.75};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    double actual_integer;
    double expected_integer;
    double actual = zenos_modf(inputs[i], &actual_integer);
    double expected = modf(inputs[i], &expected_integer);
    expect_double("modf return", i, actual, expected, DOUBLE_TOLERANCE);
    expect_double("modf integer", i, actual_integer, expected_integer, 0.0);
  }
}

Test(math, modff_matches_host) {
  const float inputs[] = {-3.75f, -1.0f, 0.0f, 1.0f, 3.75f};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    float actual_integer;
    float expected_integer;
    float actual = zenos_modff(inputs[i], &actual_integer);
    float expected = modff(inputs[i], &expected_integer);
    expect_float("modff return", i, actual, expected, FLOAT_TOLERANCE);
    expect_float("modff integer", i, actual_integer, expected_integer, 0.0f);
  }
}

Test(math, ldexp_matches_host) {
  const struct {
    double value;
    int exponent;
  } inputs[] = {{0.0, 10}, {1.0, 3}, {-1.5, 4}, {8.0, -2}};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    expect_double("ldexp", i, zenos_ldexp(inputs[i].value, inputs[i].exponent),
                  ldexp(inputs[i].value, inputs[i].exponent), 0.0);
    expect_float("ldexpf", i,
                 zenos_ldexpf((float)inputs[i].value, inputs[i].exponent),
                 ldexpf((float)inputs[i].value, inputs[i].exponent), 0.0f);
  }
}

Test(math, frexp_matches_host) {
  const double inputs[] = {-8.0, -1.5, 0.0, 1.0, 12.0};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    int actual_exponent;
    int expected_exponent;
    double actual = zenos_frexp(inputs[i], &actual_exponent);
    double expected = frexp(inputs[i], &expected_exponent);
    expect_double("frexp return", i, actual, expected, 0.0);
    cr_expect_eq(actual_exponent, expected_exponent, "frexp exponent case %zu",
                 i);

    float actual_float = zenos_frexpf((float)inputs[i], &actual_exponent);
    float expected_float = frexpf((float)inputs[i], &expected_exponent);
    expect_float("frexpf return", i, actual_float, expected_float, 0.0f);
    cr_expect_eq(actual_exponent, expected_exponent, "frexpf exponent case %zu",
                 i);
  }
}

Test(math, scalbn_matches_host) {
  const struct {
    double value;
    int exponent;
  } inputs[] = {{0.0, 10}, {1.0, 3}, {-1.5, 4}, {8.0, -2}};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    expect_double("scalbn", i,
                  zenos_scalbn(inputs[i].value, inputs[i].exponent),
                  scalbn(inputs[i].value, inputs[i].exponent), 0.0);
    expect_float("scalbnf", i,
                 zenos_scalbnf((float)inputs[i].value, inputs[i].exponent),
                 scalbnf((float)inputs[i].value, inputs[i].exponent), 0.0f);
  }
}

Test(math, ilogb_matches_host) {
  const double inputs[] = {0.125, 0.5, 1.0, 2.0, 16.0};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    cr_expect_eq(zenos_ilogb(inputs[i]), ilogb(inputs[i]),
                 "ilogb differed for case %zu", i);
    cr_expect_eq(zenos_ilogbf((float)inputs[i]), ilogbf((float)inputs[i]),
                 "ilogbf differed for case %zu", i);
  }
}

Test(math, fma_matches_host) {
  const struct {
    double x;
    double y;
    double z;
  } inputs[] = {{2.0, 3.0, 4.0}, {-2.0, 3.0, 4.0}, {0.5, 0.25, -1.0}};
  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    expect_double("fma", i, zenos_fma(inputs[i].x, inputs[i].y, inputs[i].z),
                  fma(inputs[i].x, inputs[i].y, inputs[i].z), DOUBLE_TOLERANCE);
    expect_float(
        "fmaf", i,
        zenos_fmaf((float)inputs[i].x, (float)inputs[i].y, (float)inputs[i].z),
        fmaf((float)inputs[i].x, (float)inputs[i].y, (float)inputs[i].z),
        FLOAT_TOLERANCE);
  }
}
