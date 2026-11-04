/* Measures float and decimal (34 digits), called like the generated
   C calls them. From runtime/:
       cargo rustc --release --lib --crate-type staticlib
       cc -O2 -o target/decimal-bench bench/decimal.c target/release/libkoji_runtime.a -lpthread -ldl -lm -lrt -lutil
       target/decimal-bench */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

typedef struct { uint64_t lo, hi; } D128;
D128 koji_decimal_add(D128, D128, const char *);
D128 koji_decimal_mul(D128, D128, const char *);
D128 koji_decimal_div(D128, D128, const char *);
D128 koji_decimal_from_int(int64_t);
D128 koji_decimal_make(bool, uint64_t, int32_t);
void *koji_string_from_decimal(D128);
void koji_print(void *, const char *);


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
    return koji_decimal_add(a, b, location);
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
    D128 tenth = koji_decimal_make(0, 1, -1);
    start = now(); D128 e = { 0, 0 }; for (int i = 0; i < N; i++) e = koji_decimal_add(e, tenth, at); REPORT("decimal 34 +", start);
    start = now(); D128 g = { 0, 0 }; g = koji_decimal_add(g, tenth, at); for (int i = 1; i < N; i++) g = add_inline(g, tenth, at); REPORT("decimal 34 + (inline in C)", start);

    start = now(); volatile double h = 1.0; for (int i = 0; i < N; i++) h = h * 1.0000001; REPORT("float *", start);
    D128 factor = koji_decimal_make(0, 10000001, -7);
    start = now(); D128 m = koji_decimal_from_int(1); for (int i = 0; i < N; i++) m = koji_decimal_mul(m, factor, at); REPORT("decimal 34 * (rounds)", start);
    D128 price = koji_decimal_make(0, 1999, -2), three = koji_decimal_from_int(3);
    start = now(); D128 p = price; for (int i = 0; i < N; i++) p = koji_decimal_mul(price, three, at); REPORT("decimal 34 * (exact)", start);

    start = now(); volatile double q = 1.0; for (int i = 0; i < N; i++) q = 1.0 + 1.0 / (q + 1.0); REPORT("float 1 + 1 / (x + 1)", start);
    D128 uno = koji_decimal_from_int(1);
    start = now(); D128 s = uno; for (int i = 0; i < N; i++) s = koji_decimal_add(uno, koji_decimal_div(uno, koji_decimal_add(s, uno, at), at), at); REPORT("decimal 34 1 + 1 / (x + 1)", start);
    D128 w = s, u2 = uno;
    start = now(); for (int i = 0; i < N; i++) u2 = koji_decimal_div(uno, w, at); REPORT("decimal 34 / (34 digits)", start);
    start = now(); D128 t = uno; for (int i = 0; i < N; i++) t = koji_decimal_div(price, three, at); REPORT("decimal 34 / 3 (small divisor)", start);

    koji_print(koji_string_from_decimal(e), at);
    koji_print(koji_string_from_decimal(g), at);
    koji_print(koji_string_from_decimal(m), at);
    koji_print(koji_string_from_decimal(s), at);
    (void)p; (void)t; (void)u2;
    return 0;
}
