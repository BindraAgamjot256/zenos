#pragma once

#ifdef __cplusplus
extern "C" {
#endif

#define M_E        2.71828182845904523536
#define M_LOG2E    1.44269504088896340736
#define M_LOG10E   0.434294481903251827651
#define M_LN2      0.693147180559945309417
#define M_LN10     2.30258509299404568402
#define M_PI       3.14159265358979323846
#define M_PI_2     1.57079632679489661923
#define M_PI_4     0.785398163397448309616
#define M_1_PI     0.318309886183790671538
#define M_2_PI     0.636619772367581343076
#define M_2_SQRTPI 1.12837916709551257390
#define M_SQRT2    1.41421356237309504880
#define M_SQRT1_2  0.707106781186547524401

#define HUGE_VAL   __builtin_huge_val()
#define INFINITY   __builtin_inf()
#define NAN        __builtin_nan("")

#define isnan(x)   __builtin_isnan(x)
#define isinf(x)   __builtin_isinf(x)
#define isfinite(x) __builtin_isfinite(x)

/* Basic operations */
double fabs(double x);
float fabsf(float x);

double fmod(double x, double y);
float fmodf(float x, float y);

double floor(double x);
float floorf(float x);

double ceil(double x);
float ceilf(float x);

double trunc(double x);
float truncf(float x);

double round(double x);
float roundf(float x);

/* Power and root functions */
double sqrt(double x);
float sqrtf(float x);

double pow(double base, double exp);
float powf(float base, float exp);

double cbrt(double x);
float cbrtf(float x);

double hypot(double x, double y);
float hypotf(float x, float y);

/* Exponential and logarithmic functions */
double exp(double x);
float expf(float x);

double exp2(double x);
float exp2f(float x);

double log(double x);
float logf(float x);

double log2(double x);
float log2f(float x);

double log10(double x);
float log10f(float x);

double log1p(double x);
float log1pf(float x);

double expm1(double x);
float expm1f(float x);

/* Trigonometric functions */
double sin(double x);
float sinf(float x);

double cos(double x);
float cosf(float x);

double tan(double x);
float tanf(float x);

double asin(double x);
float asinf(float x);

double acos(double x);
float acosf(float x);

double atan(double x);
float atanf(float x);

double atan2(double y, double x);
float atan2f(float y, float x);

/* Hyperbolic functions */
double sinh(double x);
float sinhf(float x);

double cosh(double x);
float coshf(float x);

double tanh(double x);
float tanhf(float x);

double asinh(double x);
float asinhf(float x);

double acosh(double x);
float acoshf(float x);

double atanh(double x);
float atanhf(float x);

/* Other functions */
double ldexp(double x, int exp);
float ldexpf(float x, int exp);

double frexp(double x, int *exp);
float frexpf(float x, int *exp);

double modf(double x, double *iptr);
float modff(float x, float *iptr);

double copysign(double x, double y);
float copysignf(float x, float y);

double fmin(double x, double y);
float fminf(float x, float y);

double fmax(double x, double y);
float fmaxf(float x, float y);

double fdim(double x, double y);
float fdimf(float x, float y);

double scalbn(double x, int n);
float scalbnf(float x, int n);

int ilogb(double x);
int ilogbf(float x);

double logb(double x);
float logbf(float x);

double nextafter(double x, double y);
float nextafterf(float x, float y);

double remainder(double x, double y);
float remainderf(float x, float y);

double fma(double x, double y, double z);
float fmaf(float x, float y, float z);

#ifdef __cplusplus
}
#endif
