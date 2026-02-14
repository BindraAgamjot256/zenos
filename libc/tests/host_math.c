/**
 * host_math.c - Host test wrapper for math.c
 *
 * Includes math.c with function names prefixed with zenos_
 * to avoid conflicts with host libc.
 */

#define fabs zenos_fabs
#define fabsf zenos_fabsf
#define copysign zenos_copysign
#define copysignf zenos_copysignf
#define floor zenos_floor
#define floorf zenos_floorf
#define ceil zenos_ceil
#define ceilf zenos_ceilf
#define trunc zenos_trunc
#define truncf zenos_truncf
#define round zenos_round
#define roundf zenos_roundf
#define fmod zenos_fmod
#define fmodf zenos_fmodf
#define modf zenos_modf
#define modff zenos_modff
#define fmin zenos_fmin
#define fminf zenos_fminf
#define fmax zenos_fmax
#define fmaxf zenos_fmaxf
#define fdim zenos_fdim
#define fdimf zenos_fdimf
#define sqrt zenos_sqrt
#define sqrtf zenos_sqrtf
#define cbrt zenos_cbrt
#define cbrtf zenos_cbrtf
#define hypot zenos_hypot
#define hypotf zenos_hypotf
#define exp zenos_exp
#define expf zenos_expf
#define exp2 zenos_exp2
#define exp2f zenos_exp2f
#define expm1 zenos_expm1
#define expm1f zenos_expm1f
#define log zenos_log
#define logf zenos_logf
#define log2 zenos_log2
#define log2f zenos_log2f
#define log10 zenos_log10
#define log10f zenos_log10f
#define log1p zenos_log1p
#define log1pf zenos_log1pf
#define pow zenos_pow
#define powf zenos_powf
#define sin zenos_sin
#define sinf zenos_sinf
#define cos zenos_cos
#define cosf zenos_cosf
#define tan zenos_tan
#define tanf zenos_tanf
#define atan zenos_atan
#define atanf zenos_atanf
#define atan2 zenos_atan2
#define atan2f zenos_atan2f
#define asin zenos_asin
#define asinf zenos_asinf
#define acos zenos_acos
#define acosf zenos_acosf
#define sinh zenos_sinh
#define sinhf zenos_sinhf
#define cosh zenos_cosh
#define coshf zenos_coshf
#define tanh zenos_tanh
#define tanhf zenos_tanhf
#define asinh zenos_asinh
#define asinhf zenos_asinhf
#define acosh zenos_acosh
#define acoshf zenos_acoshf
#define atanh zenos_atanh
#define atanhf zenos_atanhf
#define ldexp zenos_ldexp
#define ldexpf zenos_ldexpf
#define frexp zenos_frexp
#define frexpf zenos_frexpf
#define scalbn zenos_scalbn
#define scalbnf zenos_scalbnf
#define ilogb zenos_ilogb
#define ilogbf zenos_ilogbf
#define logb zenos_logb
#define logbf zenos_logbf
#define nextafter zenos_nextafter
#define nextafterf zenos_nextafterf
#define remainder zenos_remainder
#define remainderf zenos_remainderf
#define fma zenos_fma
#define fmaf zenos_fmaf

/* Use standard types and math macros from host */
#include <stdint.h>

/* Provide math constants and macros */
#define HUGE_VAL   __builtin_huge_val()
#define INFINITY   __builtin_inf()
#define NAN        __builtin_nan("")
#define isnan(x)   __builtin_isnan(x)
#define isinf(x)   __builtin_isinf(x)
#define isfinite(x) __builtin_isfinite(x)

/* Include our implementation */
#include "../src/math.c"
