/* Measures float, the decimal of 16 digits and the decimal of 34 digits, called like the generated
   C calls them. From runtime/:
       cargo rustc --release --lib --crate-type staticlib
       cc -O2 -o target/decimal-bench bench/decimal.c target/release/libkoji_runtime.a -lpthread -ldl -lm -lrt -lutil
       target/decimal-bench */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

typedef struct { uint64_t lo, hi; } D128;
uint64_t koji_decimal_add(uint64_t, uint64_t, const char *);
uint64_t koji_decimal_mul(uint64_t, uint64_t, const char *);
uint64_t koji_decimal_div(uint64_t, uint64_t, const char *);
uint64_t koji_decimal_from_int(int64_t);
D128 koji_decimal128_add(D128, D128, const char *);
D128 koji_decimal128_mul(D128, D128, const char *);
D128 koji_decimal128_div(D128, D128, const char *);
D128 koji_decimal128_from_int(int64_t);
D128 koji_decimal128_make(bool, uint64_t, int32_t);
void *koji_string_from_decimal128(D128);
void *koji_string_from_decimal(uint64_t);
void koji_print(void *, const char *);

#define KOJI_DECIMAL(negative, coefficient, exponent) \
    ((((uint64_t)((exponent) + 398)) * UINT64_C(10000000000000000) + (uint64_t)(coefficient)) * 2 + (negative))

/* The addition in C when both have the same exponent and sign and the sum fits: no call. */
#define MASK49 ((UINT64_C(1) << 49) - 1)
static const unsigned __int128 TEN34 = (unsigned __int128)UINT64_C(10000000000000000) * UINT64_C(1000000000000000000);
static inline D128 add_inline(D128 a, D128 b, const char *location) {
    if ((a.hi & ~MASK49) == (b.hi & ~MASK49)) {
        unsigned __int128 x = ((unsigned __int128)(a.hi & MASK49) << 64) | a.lo;
        unsigned __int128 y = ((unsigned __int128)(b.hi & MASK49) << 64) | b.lo;
        unsigned __int128 sum = x + y;
        if (sum < TEN34) return (D128){ (uint64_t)sum, (a.hi & ~MASK49) | (uint64_t)(sum >> 64) };
    }
    return koji_decimal128_add(a, b, location);
}

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec * 1e-9;
}

#define N 10000000
#define REPORT(name, start) printf("%-28s %7.2f ns\n", name, (now() - start) / N * 1e9)

int main(void) {
    const char *at = "bench";
    double start;

    start = now(); volatile double f = 0.0; for (int i = 0; i < N; i++) f = f + 0.1; REPORT("float +", start);
    start = now(); uint64_t d = 0; for (int i = 0; i < N; i++) d = koji_decimal_add(d, KOJI_DECIMAL(0, 1, -1), at); REPORT("decimal 16 +", start);
    D128 tenth = koji_decimal128_make(0, 1, -1);
    start = now(); D128 e = { 0, 0 }; for (int i = 0; i < N; i++) e = koji_decimal128_add(e, tenth, at); REPORT("decimal 34 +", start);
    start = now(); D128 g = { 0, 0 }; g = koji_decimal128_add(g, tenth, at); for (int i = 1; i < N; i++) g = add_inline(g, tenth, at); REPORT("decimal 34 + (inline in C)", start);

    start = now(); volatile double h = 1.0; for (int i = 0; i < N; i++) h = h * 1.0000001; REPORT("float *", start);
    start = now(); uint64_t k = KOJI_DECIMAL(0, 1, 0); for (int i = 0; i < N; i++) k = koji_decimal_mul(k, KOJI_DECIMAL(0, 10000001, -7), at); REPORT("decimal 16 * (rounds)", start);
    D128 factor = koji_decimal128_make(0, 10000001, -7);
    start = now(); D128 m = koji_decimal128_from_int(1); for (int i = 0; i < N; i++) m = koji_decimal128_mul(m, factor, at); REPORT("decimal 34 * (rounds)", start);
    D128 price = koji_decimal128_make(0, 1999, -2), three = koji_decimal128_from_int(3);
    start = now(); D128 p = price; for (int i = 0; i < N; i++) p = koji_decimal128_mul(price, three, at); REPORT("decimal 34 * (exact)", start);

    start = now(); volatile double q = 1.0; for (int i = 0; i < N; i++) q = 1.0 + 1.0 / (q + 1.0); REPORT("float 1 + 1 / (x + 1)", start);
    uint64_t one = KOJI_DECIMAL(0, 1, 0);
    start = now(); uint64_t r = one; for (int i = 0; i < N; i++) r = koji_decimal_add(one, koji_decimal_div(one, koji_decimal_add(r, one, at), at), at); REPORT("decimal 16 1 + 1 / (x + 1)", start);
    D128 uno = koji_decimal128_from_int(1);
    start = now(); D128 s = uno; for (int i = 0; i < N; i++) s = koji_decimal128_add(uno, koji_decimal128_div(uno, koji_decimal128_add(s, uno, at), at), at); REPORT("decimal 34 1 + 1 / (x + 1)", start);
    D128 w = s, u2 = uno;
    start = now(); for (int i = 0; i < N; i++) u2 = koji_decimal128_div(uno, w, at); REPORT("decimal 34 / (34 digits)", start);
    uint64_t r16 = r, v16 = one;
    start = now(); for (int i = 0; i < N; i++) v16 = koji_decimal_div(one, r16, at); REPORT("decimal 16 / (16 digits)", start);
    (void)u2; (void)v16;
    start = now(); D128 t = uno; for (int i = 0; i < N; i++) t = koji_decimal128_div(price, three, at); REPORT("decimal 34 / 3 (small divisor)", start);

    koji_print(koji_string_from_decimal(d), at);
    koji_print(koji_string_from_decimal128(e), at);
    koji_print(koji_string_from_decimal128(g), at);
    koji_print(koji_string_from_decimal128(m), at);
    koji_print(koji_string_from_decimal128(s), at);
    (void)p; (void)t; (void)k; (void)r;
    return 0;
}
