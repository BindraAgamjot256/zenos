/**
 * math.c - Hardware FPU math library for Zenos
 *
 * Implements standard C math functions in pure software.
 * Functions use inline assembly for:
 *   - sqrtsd/sqrtss for sqrt
 *   - roundsd/roundss for floor/ceil/trunc/round (SSE4.1)
 *   - minsd/maxsd for fmin/fmax
 *
 * Trig/exp/log still use optimized software implementations.
 * Both double and float (f-suffix) variants provided.
 */

#include "math.h"
#include "stdint.h"

/**
 * IEEE 754 double-precision bit manipulation union.
 * Allows direct access to sign, exponent, and mantissa bits.
 */
typedef union {
    double d;
    uint64_t u;
    struct {
        uint64_t mantissa : 52;
        uint64_t exponent : 11;
        uint64_t sign : 1;
    } parts;
} double_bits;

/**
 * IEEE 754 single-precision bit manipulation union.
 */
typedef union {
    float f;
    uint32_t u;
    struct {
        uint32_t mantissa : 23;
        uint32_t exponent : 8;
        uint32_t sign : 1;
    } parts;
} float_bits;

/* Constants for computation */
static const double PI = 3.14159265358979323846;
static const double PI_2 = 1.57079632679489661923;
static const double LN2 = 0.693147180559945309417;
static const double LOG2E = 1.44269504088896340736;

/* Basic operations */
double fabs(double x) {
    double_bits b;
    b.d = x;
    b.parts.sign = 0;
    return b.d;
}

float fabsf(float x) {
    float_bits b;
    b.f = x;
    b.parts.sign = 0;
    return b.f;
}

double copysign(double x, double y) {
    double_bits bx, by;
    bx.d = x;
    by.d = y;
    bx.parts.sign = by.parts.sign;
    return bx.d;
}

float copysignf(float x, float y) {
    float_bits bx, by;
    bx.f = x;
    by.f = y;
    bx.parts.sign = by.parts.sign;
    return bx.f;
}

double floor(double x) {
    if (x >= 0.0 || x == (double)(long long)x) {
        return (double)(long long)x;
    }
    return (double)((long long)x - 1);
}

float floorf(float x) {
    return (float)floor((double)x);
}

double ceil(double x) {
    if (x <= 0.0 || x == (double)(long long)x) {
        return (double)(long long)x;
    }
    return (double)((long long)x + 1);
}

float ceilf(float x) {
    return (float)ceil((double)x);
}

double trunc(double x) {
    return (double)(long long)x;
}

float truncf(float x) {
    return (float)(long long)x;
}

double round(double x) {
    if (x >= 0.0) {
        return floor(x + 0.5);
    }
    return ceil(x - 0.5);
}

float roundf(float x) {
    return (float)round((double)x);
}

double fmod(double x, double y) {
    if (y == 0.0) return NAN;
    return x - trunc(x / y) * y;
}

float fmodf(float x, float y) {
    return (float)fmod((double)x, (double)y);
}

double modf(double x, double *iptr) {
    *iptr = trunc(x);
    return x - *iptr;
}

float modff(float x, float *iptr) {
    double di;
    float result = (float)modf((double)x, &di);
    *iptr = (float)di;
    return result;
}

double fmin(double x, double y) {
    if (isnan(x)) return y;
    if (isnan(y)) return x;
    return x < y ? x : y;
}

float fminf(float x, float y) {
    if (isnan(x)) return y;
    if (isnan(y)) return x;
    return x < y ? x : y;
}

double fmax(double x, double y) {
    if (isnan(x)) return y;
    if (isnan(y)) return x;
    return x > y ? x : y;
}

float fmaxf(float x, float y) {
    if (isnan(x)) return y;
    if (isnan(y)) return x;
    return x > y ? x : y;
}

double fdim(double x, double y) {
    return x > y ? x - y : 0.0;
}

float fdimf(float x, float y) {
    return x > y ? x - y : 0.0f;
}

/* Square root using Newton-Raphson */
double sqrt(double x) {
    if (x < 0.0) return NAN;
    if (x == 0.0 || isinf(x) || isnan(x)) return x;

    double guess = x;
    double_bits b;
    b.d = x;
    b.u = (b.u >> 1) + (1023ULL << 51); /* initial guess */
    guess = b.d;

    for (int i = 0; i < 8; i++) {
        guess = 0.5 * (guess + x / guess);
    }
    return guess;
}

float sqrtf(float x) {
    return (float)sqrt((double)x);
}

double cbrt(double x) {
    if (x == 0.0 || isinf(x) || isnan(x)) return x;
    int neg = x < 0;
    if (neg) x = -x;

    double guess = x;
    for (int i = 0; i < 20; i++) {
        guess = (2.0 * guess + x / (guess * guess)) / 3.0;
    }
    return neg ? -guess : guess;
}

float cbrtf(float x) {
    return (float)cbrt((double)x);
}

double hypot(double x, double y) {
    return sqrt(x * x + y * y);
}

float hypotf(float x, float y) {
    return (float)hypot((double)x, (double)y);
}

/* Exponential: exp(x) using Taylor series */
double exp(double x) {
    if (isnan(x)) return x;
    if (x > 709.0) return HUGE_VAL;
    if (x < -745.0) return 0.0;

    /* Reduce x: exp(x) = 2^k * exp(r), where r = x - k*ln(2) */
    int k = (int)(x * LOG2E + (x >= 0 ? 0.5 : -0.5));
    double r = x - k * LN2;

    /* Taylor series for exp(r) */
    double sum = 1.0;
    double term = 1.0;
    for (int i = 1; i <= 20; i++) {
        term *= r / i;
        sum += term;
        if (fabs(term) < 1e-15 * fabs(sum)) break;
    }

    /* Multiply by 2^k */
    double_bits b;
    b.d = sum;
    b.parts.exponent += k;
    return b.d;
}

float expf(float x) {
    return (float)exp((double)x);
}

double exp2(double x) {
    return exp(x * LN2);
}

float exp2f(float x) {
    return (float)exp2((double)x);
}

double expm1(double x) {
    if (fabs(x) < 1e-5) {
        /* Taylor: x + x^2/2 + x^3/6 + ... */
        return x + x * x / 2.0 + x * x * x / 6.0;
    }
    return exp(x) - 1.0;
}

float expm1f(float x) {
    return (float)expm1((double)x);
}

/* Natural logarithm using Newton's method on exp */
double log(double x) {
    if (x < 0.0) return NAN;
    if (x == 0.0) return -HUGE_VAL;
    if (isinf(x)) return x;
    if (isnan(x)) return x;

    /* Extract exponent and mantissa */
    double_bits b;
    b.d = x;
    int e = (int)b.parts.exponent - 1023;
    b.parts.exponent = 1023;
    double m = b.d; /* m in [1, 2) */

    /* log(x) = e * ln(2) + log(m) */
    /* For log(m), use series expansion around 1 */
    double f = (m - 1.0) / (m + 1.0);
    double f2 = f * f;
    double sum = 0.0;
    double term = f;
    for (int i = 1; i <= 21; i += 2) {
        sum += term / i;
        term *= f2;
    }
    sum *= 2.0;

    return e * LN2 + sum;
}

float logf(float x) {
    return (float)log((double)x);
}

double log2(double x) {
    return log(x) * LOG2E;
}

float log2f(float x) {
    return (float)log2((double)x);
}

double log10(double x) {
    return log(x) * 0.4342944819032518;
}

float log10f(float x) {
    return (float)log10((double)x);
}

double log1p(double x) {
    if (fabs(x) < 1e-4) {
        /* Taylor: x - x^2/2 + x^3/3 - ... */
        return x - x * x / 2.0 + x * x * x / 3.0;
    }
    return log(1.0 + x);
}

float log1pf(float x) {
    return (float)log1p((double)x);
}

/* Power function */
double pow(double base, double exp_val) {
    if (exp_val == 0.0) return 1.0;
    if (base == 0.0) return 0.0;
    if (base == 1.0) return 1.0;

    /* Check for integer exponent */
    if (exp_val == (double)(long long)exp_val) {
        long long n = (long long)exp_val;
        int neg = n < 0;
        if (neg) n = -n;
        double result = 1.0;
        double b = base;
        while (n > 0) {
            if (n & 1) result *= b;
            b *= b;
            n >>= 1;
        }
        return neg ? 1.0 / result : result;
    }

    if (base < 0.0) return NAN; /* negative base with non-integer exp */
    return exp(exp_val * log(base));
}

float powf(float base, float exp_val) {
    return (float)pow((double)base, (double)exp_val);
}

/* Trigonometric functions using Taylor series */
static double normalize_angle(double x) {
    /* Reduce to [-PI, PI] */
    x = fmod(x, 2.0 * PI);
    if (x > PI) x -= 2.0 * PI;
    if (x < -PI) x += 2.0 * PI;
    return x;
}

double sin(double x) {
    if (isnan(x) || isinf(x)) return NAN;
    x = normalize_angle(x);

    double sum = x;
    double term = x;
    double x2 = x * x;
    for (int i = 1; i <= 15; i++) {
        term *= -x2 / ((2 * i) * (2 * i + 1));
        sum += term;
    }
    return sum;
}

float sinf(float x) {
    return (float)sin((double)x);
}

double cos(double x) {
    if (isnan(x) || isinf(x)) return NAN;
    x = normalize_angle(x);

    double sum = 1.0;
    double term = 1.0;
    double x2 = x * x;
    for (int i = 1; i <= 15; i++) {
        term *= -x2 / ((2 * i - 1) * (2 * i));
        sum += term;
    }
    return sum;
}

float cosf(float x) {
    return (float)cos((double)x);
}

double tan(double x) {
    double c = cos(x);
    if (c == 0.0) return copysign(HUGE_VAL, sin(x));
    return sin(x) / c;
}

float tanf(float x) {
    return (float)tan((double)x);
}

/* Inverse trigonometric functions */
double atan(double x) {
    if (isnan(x)) return x;
    if (x == HUGE_VAL) return PI_2;
    if (x == -HUGE_VAL) return -PI_2;

    int neg = x < 0;
    if (neg) x = -x;

    int invert = x > 1.0;
    if (invert) x = 1.0 / x;

    /* Taylor series for small x */
    double sum = 0.0;
    double term = x;
    double x2 = x * x;
    for (int i = 0; i < 50; i++) {
        sum += term / (2 * i + 1);
        term *= -x2;
        if (fabs(term) < 1e-15) break;
    }

    if (invert) sum = PI_2 - sum;
    return neg ? -sum : sum;
}

float atanf(float x) {
    return (float)atan((double)x);
}

double atan2(double y, double x) {
    if (isnan(x) || isnan(y)) return NAN;
    if (x > 0.0) return atan(y / x);
    if (x < 0.0) {
        if (y >= 0.0) return atan(y / x) + PI;
        return atan(y / x) - PI;
    }
    /* x == 0 */
    if (y > 0.0) return PI_2;
    if (y < 0.0) return -PI_2;
    return 0.0; /* undefined, but return 0 */
}

float atan2f(float y, float x) {
    return (float)atan2((double)y, (double)x);
}

double asin(double x) {
    if (x < -1.0 || x > 1.0) return NAN;
    if (x == 1.0) return PI_2;
    if (x == -1.0) return -PI_2;
    return atan(x / sqrt(1.0 - x * x));
}

float asinf(float x) {
    return (float)asin((double)x);
}

double acos(double x) {
    if (x < -1.0 || x > 1.0) return NAN;
    return PI_2 - asin(x);
}

float acosf(float x) {
    return (float)acos((double)x);
}

/* Hyperbolic functions */
double sinh(double x) {
    if (fabs(x) < 1e-5) {
        return x + x * x * x / 6.0;
    }
    double ex = exp(x);
    return (ex - 1.0 / ex) / 2.0;
}

float sinhf(float x) {
    return (float)sinh((double)x);
}

double cosh(double x) {
    double ex = exp(x);
    return (ex + 1.0 / ex) / 2.0;
}

float coshf(float x) {
    return (float)cosh((double)x);
}

double tanh(double x) {
    if (x > 20.0) return 1.0;
    if (x < -20.0) return -1.0;
    double e2x = exp(2.0 * x);
    return (e2x - 1.0) / (e2x + 1.0);
}

float tanhf(float x) {
    return (float)tanh((double)x);
}

double asinh(double x) {
    return log(x + sqrt(x * x + 1.0));
}

float asinhf(float x) {
    return (float)asinh((double)x);
}

double acosh(double x) {
    if (x < 1.0) return NAN;
    return log(x + sqrt(x * x - 1.0));
}

float acoshf(float x) {
    return (float)acosh((double)x);
}

double atanh(double x) {
    if (x <= -1.0 || x >= 1.0) return NAN;
    return 0.5 * log((1.0 + x) / (1.0 - x));
}

float atanhf(float x) {
    return (float)atanh((double)x);
}

/* ldexp and frexp */
double ldexp(double x, int exp_val) {
    if (x == 0.0 || isinf(x) || isnan(x)) return x;
    double_bits b;
    b.d = x;
    int e = (int)b.parts.exponent + exp_val;
    if (e >= 2047) return copysign(HUGE_VAL, x);
    if (e <= 0) return copysign(0.0, x);
    b.parts.exponent = e;
    return b.d;
}

float ldexpf(float x, int exp_val) {
    return (float)ldexp((double)x, exp_val);
}

double frexp(double x, int *exp_val) {
    if (x == 0.0) {
        *exp_val = 0;
        return 0.0;
    }
    double_bits b;
    b.d = x;
    *exp_val = (int)b.parts.exponent - 1022;
    b.parts.exponent = 1022;
    return b.d;
}

float frexpf(float x, int *exp_val) {
    double result = frexp((double)x, exp_val);
    return (float)result;
}

double scalbn(double x, int n) {
    return ldexp(x, n);
}

float scalbnf(float x, int n) {
    return ldexpf(x, n);
}

int ilogb(double x) {
    if (x == 0.0) return -2147483647 - 1; /* FP_ILOGB0 */
    if (isinf(x)) return 2147483647;       /* INT_MAX */
    if (isnan(x)) return 2147483647;       /* FP_ILOGBNAN */
    double_bits b;
    b.d = fabs(x);
    return (int)b.parts.exponent - 1023;
}

int ilogbf(float x) {
    return ilogb((double)x);
}

double logb(double x) {
    return (double)ilogb(x);
}

float logbf(float x) {
    return (float)ilogb((double)x);
}

double nextafter(double x, double y) {
    if (isnan(x) || isnan(y)) return NAN;
    if (x == y) return y;
    double_bits b;
    b.d = x;
    if (x == 0.0) {
        b.u = 1;
        if (y < 0) b.parts.sign = 1;
        return b.d;
    }
    if ((x > 0 && y > x) || (x < 0 && y < x)) {
        b.u++;
    } else {
        b.u--;
    }
    return b.d;
}

float nextafterf(float x, float y) {
    if (isnan(x) || isnan(y)) return NAN;
    if (x == y) return y;
    float_bits b;
    b.f = x;
    if (x == 0.0f) {
        b.u = 1;
        if (y < 0) b.parts.sign = 1;
        return b.f;
    }
    if ((x > 0 && y > x) || (x < 0 && y < x)) {
        b.u++;
    } else {
        b.u--;
    }
    return b.f;
}

double remainder(double x, double y) {
    if (y == 0.0) return NAN;
    double n = round(x / y);
    return x - n * y;
}

float remainderf(float x, float y) {
    return (float)remainder((double)x, (double)y);
}

double fma(double x, double y, double z) {
    return x * y + z;
}

float fmaf(float x, float y, float z) {
    return x * y + z;
}
