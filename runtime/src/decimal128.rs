//! A decimal with 34 significant digits (the precision of IEEE 754 decimal128), computed in
//! software, for measuring it against the decimal of 16 digits (`decimal.rs`).
//!
//! A value is `coefficient * 10^exponent`, with a coefficient below 10^34 and an exponent from
//! -6176 to 6111. It is not normalized (`1.50` may be kept as 150e-2): operations skip the work of
//! removing trailing zeros, and equality, hashing and text look through them. The common cases
//! need no division: adding values with the same exponent, and results that are exact.
//!
//! In C a value is two 64-bit words, passed in registers: `lo` is the low 64 bits of the
//! coefficient; `hi` has the 49 high bits of the coefficient, then 14 bits of exponent (plus
//! 6176), then the sign. Zero is all zero bits.

// Not used by the compiler yet: measured first (see the plan).
#![allow(dead_code)]

use std::cmp::Ordering;
use std::ffi::c_char;

use crate::output::panic_at;

const DIGITS: u32 = 34;
const MIN_EXPONENT: i64 = -6176;
const MAX_EXPONENT: i64 = 6111;

const fn powers() -> [u128; 39] {
    let mut table = [1u128; 39];
    let mut i = 1;
    while i < 39 {
        table[i] = table[i - 1] * 10;
        i += 1;
    }
    table
}

static POW10: [u128; 39] = POW10_CONST;
const TEN_34: u128 = 10_000_000_000_000_000_000_000_000_000_000_000;

/// The number of decimal digits, without dividing: from the number of bits and a table.
#[inline]
fn digits(n: u128) -> u32 {
    if n == 0 {
        return 1;
    }
    let bits = 128 - n.leading_zeros();
    // floor(bits * log10(2)), then one more when n reaches the next power of ten.
    let guess = (bits * 1233) >> 12;
    guess + (n >= POW10[guess as usize]) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal128 {
    pub negative: bool,
    pub coefficient: u128,
    pub exponent: i64,
}

pub use crate::decimal::Limit;

const ZERO: Decimal128 = Decimal128 { negative: false, coefficient: 0, exponent: MIN_EXPONENT };

/// Rounds `coefficient * 10^exponent` (plus less than one unit of its last digit when `sticky`)
/// to 34 digits, half to even. The coefficient may have up to 39 digits.
#[inline]
fn finish(negative: bool, coefficient: u128, exponent: i64, sticky: bool) -> Result<Decimal128, Limit> {
    // Most results: exact, with 34 digits or less, in range.
    if !sticky && coefficient < TEN_34 && (MIN_EXPONENT..=MAX_EXPONENT).contains(&exponent) {
        return Ok(Decimal128 { negative: negative && coefficient != 0, coefficient, exponent });
    }
    slow_finish(negative, coefficient, exponent, sticky)
}

#[cold]
fn slow_finish(negative: bool, coefficient: u128, exponent: i64, sticky: bool) -> Result<Decimal128, Limit> {
    if coefficient == 0 && !sticky {
        return Ok(Decimal128 { exponent: exponent.clamp(MIN_EXPONENT, MAX_EXPONENT), ..ZERO });
    }
    let count = digits(coefficient) as i64;
    let drop = (count - DIGITS as i64).max(MIN_EXPONENT - exponent).max(0);
    let (mut kept, mut exponent) = (coefficient, exponent);
    if drop > 0 {
        let (quotient, rest, unit) = if drop >= 39 {
            (0, coefficient, None)
        } else if drop <= 19 {
            let unit = POW10[drop as usize];
            let (quotient, rest) = divide_small(coefficient, unit as u64);
            (quotient, rest as u128, Some(unit))
        } else {
            let unit = POW10[drop as usize];
            let quotient = coefficient / unit;
            (quotient, coefficient - quotient * unit, Some(unit))
        };
        // Beyond 38 digits, the rest is below half a unit.
        let up = match unit {
            Some(unit) => {
                let half = unit / 2;
                rest > half || (rest == half && (sticky || quotient % 2 == 1))
            }
            None => false,
        };
        kept = quotient + up as u128;
        exponent += drop;
        if kept == TEN_34 {
            kept /= 10;
            exponent += 1;
        }
        if kept == 0 {
            return Err(Limit::Underflow);
        }
    }
    if exponent > MAX_EXPONENT {
        // A value with fewer digits can take zeros instead of a larger exponent.
        let pad = exponent - MAX_EXPONENT;
        if digits(kept) as i64 + pad > DIGITS as i64 {
            return Err(Limit::Overflow);
        }
        kept *= POW10[pad as usize];
        exponent = MAX_EXPONENT;
    }
    Ok(Decimal128 { negative, coefficient: kept, exponent })
}

// Numbers of 256 bits, for products and divisions: four 64-bit words, least significant first.

type Wide = [u64; 4];

#[inline]
const fn wide_mul(a: u128, b: u128) -> Wide {
    let (a0, a1) = (a as u64 as u128, (a >> 64) as u64 as u128);
    let (b0, b1) = (b as u64 as u128, (b >> 64) as u64 as u128);
    let low = a0 * b0;
    let middle1 = a0 * b1;
    let middle2 = a1 * b0;
    let high = a1 * b1;
    let (middle, carry) = middle1.overflowing_add(middle2);
    let (low, carry_low) = low.overflowing_add(middle << 64);
    let high = high + (middle >> 64) + ((carry as u128) << 64) + carry_low as u128;
    [low as u64, (low >> 64) as u64, high as u64, (high >> 64) as u64]
}

const fn wide_powers() -> [Wide; 40] {
    let mut table = [[0u64; 4]; 40];
    let mut i = 0;
    while i < 39 {
        table[i] = wide_mul(POW10_CONST[38], POW10_CONST[i]);
        i += 1;
    }
    // 10^77 = 10^76 * 10
    let mut carry = 0u128;
    let mut word = 0;
    let last = wide_mul(POW10_CONST[38], POW10_CONST[38]);
    while word < 4 {
        let product = last[word] as u128 * 10 + carry;
        table[39][word] = product as u64;
        carry = product >> 64;
        word += 1;
    }
    table
}

const POW10_CONST: [u128; 39] = powers();

/// 10^(38 + i), for the numbers of 256 bits.
static POW10_WIDE: [Wide; 40] = wide_powers();

/// `(high * 2^64 + low) / d` and the remainder, for `high < d`: one hardware division.
#[inline]
fn divide_word(high: u64, low: u64, d: u64) -> (u64, u64) {
    debug_assert!(high < d);
    #[cfg(target_arch = "x86_64")]
    {
        let quotient: u64;
        let rest: u64;
        // SAFETY: `high < d`, so the quotient fits in 64 bits and `div` does not fault.
        unsafe {
            std::arch::asm!("div {d}", d = in(reg) d, inout("rax") low => quotient, inout("rdx") high => rest, options(pure, nomem, nostack));
        }
        (quotient, rest)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let n = (high as u128) << 64 | low as u128;
        ((n / d as u128) as u64, (n % d as u128) as u64)
    }
}

/// `n / d` and `n % d` for a divisor of 64 bits.
#[inline]
fn divide_small(n: u128, d: u64) -> (u128, u64) {
    let (high, low) = ((n >> 64) as u64, n as u64);
    let (q1, r1) = (high / d, high % d);
    let (q0, rest) = divide_word(r1, low, d);
    ((q1 as u128) << 64 | q0 as u128, rest)
}

fn narrow(n: &Wide) -> Option<u128> {
    if n[2] == 0 && n[3] == 0 {
        Some(n[0] as u128 | (n[1] as u128) << 64)
    } else {
        None
    }
}

/// `n / d` and `n % d`: Knuth's algorithm D with 64-bit digits (the divisor has one or two), so
/// each step of the quotient is one hardware division.
fn wide_divide(n: &Wide, d: u128) -> (Wide, u128) {
    assert!(d != 0, "division by zero");
    let m = n.iter().rposition(|&x| x != 0).map_or(0, |i| i + 1);
    let (d0, d1) = (d as u64, (d >> 64) as u64);
    if d1 == 0 {
        // A divisor of one digit: short division.
        let mut quotient = [0u64; 4];
        let mut rest = 0u64;
        for i in (0..m).rev() {
            let (digit, remainder) = divide_word(rest, n[i], d0);
            quotient[i] = digit;
            rest = remainder;
        }
        return (quotient, rest as u128);
    }
    if m < 2 || narrow(n).is_some_and(|small| small < d) {
        return ([0; 4], narrow(n).unwrap_or(0));
    }
    let shift = d1.leading_zeros();
    let shl = |high: u64, low: u64| if shift == 0 { high } else { high << shift | low >> (64 - shift) };
    let (v0, v1) = (d0 << shift, shl(d1, d0));
    let mut u = [0u64; 5];
    u[m] = if shift == 0 { 0 } else { n[m - 1] >> (64 - shift) };
    for i in (1..m).rev() {
        u[i] = shl(n[i], n[i - 1]);
    }
    u[0] = n[0] << shift;
    let mut quotient = [0u64; 4];
    for j in (0..=m - 2).rev() {
        let top = (u[j + 2] as u128) << 64 | u[j + 1] as u128;
        let (mut estimate, mut remainder) = if u[j + 2] < v1 {
            let (q, r) = divide_word(u[j + 2], u[j + 1], v1);
            (q as u128, r as u128)
        } else {
            let q = u64::MAX as u128;
            (q, top - q * v1 as u128)
        };
        // At most two corrections; a remainder of 65 bits means the estimate is right.
        while remainder >> 64 == 0 && estimate * v0 as u128 > (remainder << 64 | u[j] as u128) {
            estimate -= 1;
            remainder += v1 as u128;
        }
        // u[j..j+3] -= estimate * (v1, v0)
        let low = estimate * v0 as u128;
        let high = estimate * v1 as u128 + (low >> 64);
        let (a, borrow_a) = u[j].overflowing_sub(low as u64);
        let (b, borrow_b1) = u[j + 1].overflowing_sub(high as u64);
        let (b, borrow_b2) = b.overflowing_sub(borrow_a as u64);
        let (c, borrow_c1) = u[j + 2].overflowing_sub((high >> 64) as u64);
        let (c, borrow_c2) = c.overflowing_sub((borrow_b1 || borrow_b2) as u64);
        u[j] = a;
        u[j + 1] = b;
        u[j + 2] = c;
        quotient[j] = estimate as u64;
        if borrow_c1 || borrow_c2 {
            // The estimate was one too large: add the divisor back.
            quotient[j] -= 1;
            let (a, carry_a) = u[j].overflowing_add(v0);
            let (b, carry_b1) = u[j + 1].overflowing_add(v1);
            let (b, carry_b2) = b.overflowing_add(carry_a as u64);
            u[j] = a;
            u[j + 1] = b;
            u[j + 2] = u[j + 2].wrapping_add((carry_b1 || carry_b2) as u64);
        }
    }
    let rest = if shift == 0 {
        u[0] as u128 | (u[1] as u128) << 64
    } else {
        (u[0] >> shift | u[1] << (64 - shift)) as u128 | ((u[1] >> shift | u[2] << (64 - shift)) as u128) << 64
    };
    (quotient, rest)
}

/// The number of digits of a number of 256 bits, from its number of bits and one comparison.
fn wide_digits(n: &Wide) -> u32 {
    if let Some(small) = narrow(n) {
        return digits(small);
    }
    let top = n.iter().rposition(|&x| x != 0).unwrap_or(0);
    let bits = top as u32 * 64 + 64 - n[top].leading_zeros();
    let guess = (bits * 1233) >> 12;
    // 10^guess: guess is from 38 (above 2^128) to 77.
    guess + !wide_below(n, &POW10_WIDE[(guess - 38) as usize]) as u32
}

fn wide_below(a: &Wide, b: &Wide) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    false
}

/// `finish` for a coefficient of up to 77 digits: cut to 38 digits (the rest as `sticky`), which
/// `finish` rounds.
fn finish_wide(negative: bool, coefficient: Wide, exponent: i64, sticky: bool) -> Result<Decimal128, Limit> {
    if let Some(small) = narrow(&coefficient) {
        return finish(negative, small, exponent, sticky);
    }
    // Usually: one division keeps 34 digits, and its remainder rounds them.
    let drop = wide_digits(&coefficient) as i64 - DIGITS as i64;
    if drop <= 38 && exponent + drop >= MIN_EXPONENT && exponent + drop <= MAX_EXPONENT {
        let unit = POW10[drop as usize];
        let (quotient, rest) = wide_divide(&coefficient, unit);
        let quotient = narrow(&quotient).unwrap_or(u128::MAX);
        let half = unit / 2;
        let up = rest > half || (rest == half && (sticky || quotient % 2 == 1));
        let kept = quotient + up as u128;
        if kept < TEN_34 {
            return Ok(Decimal128 { negative, coefficient: kept, exponent: exponent + drop });
        }
        return finish(negative, kept / 10, exponent + drop + 1, false);
    }
    let (kept, exponent, sticky) = cut(coefficient, exponent, sticky);
    finish(negative, kept, exponent, sticky)
}

/// The number with 38 digits at most, and whether a nonzero part was cut.
fn cut(coefficient: Wide, exponent: i64, sticky: bool) -> (u128, i64, bool) {
    if let Some(small) = narrow(&coefficient) {
        return (small, exponent, sticky);
    }
    let mut drop = wide_digits(&coefficient) - 38;
    let (mut value, mut exponent, mut sticky) = (coefficient, exponent, sticky);
    while drop > 0 {
        let step = drop.min(38);
        let (quotient, rest) = wide_divide(&value, POW10[step as usize]);
        value = quotient;
        sticky |= rest != 0;
        exponent += step as i64;
        drop -= step;
    }
    (narrow(&value).unwrap_or(u128::MAX), exponent, sticky)
}

impl Decimal128 {
    pub fn from_bits(lo: u64, hi: u64) -> Decimal128 {
        let coefficient = lo as u128 | ((hi & ((1 << 49) - 1)) as u128) << 64;
        let exponent = ((hi >> 49) & 0x3FFF) as i64 + MIN_EXPONENT;
        Decimal128 { negative: hi >> 63 == 1, coefficient, exponent }
    }

    pub fn to_bits(self) -> (u64, u64) {
        let hi = (self.coefficient >> 64) as u64 | ((self.exponent - MIN_EXPONENT) as u64) << 49 | (self.negative as u64) << 63;
        (self.coefficient as u64, hi)
    }

    pub fn is_zero(self) -> bool {
        self.coefficient == 0
    }

    pub fn new(negative: bool, coefficient: u128, exponent: i64) -> Result<Decimal128, Limit> {
        finish(negative, coefficient, exponent, false)
    }

    pub fn from_int(value: i64) -> Decimal128 {
        Decimal128 { negative: value < 0, coefficient: value.unsigned_abs() as u128, exponent: 0 }
    }

    pub fn negate(self) -> Decimal128 {
        if self.is_zero() {
            self
        } else {
            Decimal128 { negative: !self.negative, ..self }
        }
    }

    #[inline]
    pub fn add(self, other: Decimal128) -> Result<Decimal128, Limit> {
        if self.exponent == other.exponent {
            // The same exponent (amounts of money, measures): one addition, no alignment.
            return if self.negative == other.negative {
                finish(self.negative, self.coefficient + other.coefficient, self.exponent, false)
            } else if self.coefficient >= other.coefficient {
                finish(self.negative, self.coefficient - other.coefficient, self.exponent, false)
            } else {
                finish(other.negative, other.coefficient - self.coefficient, self.exponent, false)
            };
        }
        if self.is_zero() {
            return Ok(other);
        }
        if other.is_zero() {
            return Ok(self);
        }
        self.add_aligned(other)
    }

    fn add_aligned(self, other: Decimal128) -> Result<Decimal128, Limit> {
        let (a, b) = if self.exponent > other.exponent { (self, other) } else { (other, self) };
        let distance = (a.exponent - b.exponent) as u64;
        let (big, small, exponent, sticky) = if distance <= 38 && a.coefficient < POW10[38 - distance as usize] {
            // Exact: `a` shifted still fits in 128 bits.
            (a.coefficient * POW10[distance as usize], b.coefficient, b.exponent, false)
        } else {
            // `b` is far below: `a` with 38 digits, and `b` cut, the rest of it as `sticky`.
            let shift_a = 38 - digits(a.coefficient) as u64;
            let shift_b = distance - shift_a;
            let (small, sticky) = if shift_b >= 39 {
                (0, b.coefficient != 0)
            } else {
                let unit = POW10[shift_b as usize];
                (b.coefficient / unit, b.coefficient % unit != 0)
            };
            (a.coefficient * POW10[shift_a as usize], small, a.exponent - shift_a as i64, sticky)
        };
        if a.negative == b.negative {
            return finish(a.negative, big + small, exponent, sticky);
        }
        if big >= small {
            finish(a.negative, big - small - sticky as u128, exponent, sticky)
        } else {
            finish(b.negative, small - big, exponent, false)
        }
    }

    pub fn subtract(self, other: Decimal128) -> Result<Decimal128, Limit> {
        self.add(other.negate())
    }

    #[inline]
    pub fn multiply(self, other: Decimal128) -> Result<Decimal128, Limit> {
        let negative = self.negative != other.negative;
        let exponent = self.exponent + other.exponent;
        if self.is_zero() || other.is_zero() {
            return Ok(ZERO);
        }
        if self.coefficient >> 64 == 0 && other.coefficient >> 64 == 0 {
            // Both below 2^64: the product fits in 128 bits.
            return finish(negative, self.coefficient * other.coefficient, exponent, false);
        }
        finish_wide(negative, wide_mul(self.coefficient, other.coefficient), exponent, false)
    }

    /// `None` when `other` is zero.
    pub fn divide(self, other: Decimal128) -> Option<Result<Decimal128, Limit>> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(Ok(ZERO));
        }
        let negative = self.negative != other.negative;
        // Scaled so that the quotient has at least 35 digits (one to round with).
        let scale = (35 + digits(other.coefficient) as i64 - digits(self.coefficient) as i64).max(0);
        let exponent = self.exponent - other.exponent - scale;
        if digits(self.coefficient) as i64 + scale <= 38 {
            // A small divisor: one division of 128 bits.
            let dividend = self.coefficient * POW10[scale as usize];
            let (quotient, rest) = if other.coefficient >> 64 == 0 {
                let (quotient, rest) = divide_small(dividend, other.coefficient as u64);
                (quotient, rest as u128)
            } else {
                let quotient = dividend / other.coefficient;
                (quotient, dividend - quotient * other.coefficient)
            };
            return Some(finish(negative, quotient, exponent, rest != 0));
        }
        let dividend = scaled(self.coefficient, scale as u32);
        let (quotient, rest) = wide_divide(&dividend, other.coefficient);
        Some(finish_wide(negative, quotient, exponent, rest != 0))
    }

    /// The remainder of the division truncated toward zero, with the sign of `self`; exact.
    pub fn remainder(self, other: Decimal128) -> Option<Decimal128> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(ZERO);
        }
        let (a, b) = (self.coefficient, other.coefficient);
        let (rest, exponent) = if self.exponent >= other.exponent {
            // (a * 10^d) mod b, with products of 256 bits.
            let mul_mod = |x: u128, y: u128| wide_divide(&wide_mul(x, y), b).1;
            let mut power = 1 % b;
            let mut base = 10 % b;
            let mut d = (self.exponent - other.exponent) as u64;
            while d > 0 {
                if d & 1 == 1 {
                    power = mul_mod(power, base);
                }
                base = mul_mod(base, base);
                d >>= 1;
            }
            (mul_mod(a % b, power), other.exponent)
        } else {
            let d = (other.exponent - self.exponent) as usize;
            // b * 10^d is above a (below 10^34) once it has 35 digits.
            let rest = if d + digits(b) as usize > 34 { a } else { a % (b * POW10[d]) };
            (rest, self.exponent)
        };
        Some(finish(self.negative, rest, exponent, false).unwrap_or(ZERO))
    }

    /// `self ** exponent`; `None` for zero to a negative power.
    pub fn power(self, exponent: i64) -> Option<Result<Decimal128, Limit>> {
        if exponent == 0 {
            return Some(Ok(Decimal128::from_int(1)));
        }
        if self.is_zero() {
            return if exponent < 0 { None } else { Some(Ok(ZERO)) };
        }
        let negative = self.negative && exponent % 2 != 0;
        // Squaring with 38 digits (four more than the result keeps), then one rounding.
        let mut result = Kept { coefficient: 1, exponent: 0, sticky: false };
        let mut base = Kept { coefficient: self.coefficient, exponent: self.exponent, sticky: false };
        let mut n = exponent.unsigned_abs();
        while n > 0 {
            if n & 1 == 1 {
                result = result.multiply(base);
            }
            n >>= 1;
            if n > 0 {
                base = base.multiply(base);
            }
            let position = |kept: &Kept| digits(kept.coefficient) as i64 + kept.exponent;
            if position(&result).max(position(&base)) > 2 * (MAX_EXPONENT + 40) {
                return Some(Err(if exponent > 0 { Limit::Overflow } else { Limit::Underflow }));
            }
            if position(&result).min(position(&base)) < 2 * MIN_EXPONENT {
                return Some(Err(if exponent > 0 { Limit::Underflow } else { Limit::Overflow }));
            }
        }
        if exponent > 0 {
            return Some(finish(negative, result.coefficient, result.exponent, result.sticky));
        }
        // 1 / (coefficient * 10^exponent), the divisor with up to 38 digits.
        let scale = 35 + digits(result.coefficient);
        let (quotient, rest) = wide_divide(&scaled(1, scale), result.coefficient);
        Some(finish_wide(negative, quotient, -result.exponent - scale as i64, rest != 0 || result.sticky))
    }

    /// The square root, correctly rounded; `None` for a negative number.
    pub fn sqrt(self) -> Option<Decimal128> {
        if self.negative && !self.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(ZERO);
        }
        // The coefficient with 69 or 70 digits and an even exponent: its root has 35 digits.
        let mut shift = 70 - digits(self.coefficient) as i64;
        if (self.exponent - shift) % 2 != 0 {
            shift -= 1;
        }
        let scaled = scaled(self.coefficient, shift as u32);
        // Newton's method from just above the root of a float (53 correct bits): it goes down to
        // the floor of the root.
        let estimate = wide_to_float(&scaled).sqrt() as u128;
        let mut root = estimate + (estimate >> 40) + 2;
        loop {
            let (quotient, _) = wide_divide(&scaled, root);
            let next = (root + narrow(&quotient).unwrap_or(u128::MAX)) / 2;
            if next >= root {
                break;
            }
            root = next;
        }
        let sticky = wide_mul(root, root) != scaled;
        Some(finish(false, root, (self.exponent - shift) / 2, sticky).unwrap_or(ZERO))
    }

    pub fn compare(self, other: Decimal128) -> Ordering {
        match (self.is_zero(), other.is_zero()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return if other.negative { Ordering::Greater } else { Ordering::Less },
            (false, true) => return if self.negative { Ordering::Less } else { Ordering::Greater },
            _ => {}
        }
        if self.negative != other.negative {
            return if self.negative { Ordering::Less } else { Ordering::Greater };
        }
        let magnitude = if self.exponent == other.exponent {
            self.coefficient.cmp(&other.coefficient)
        } else {
            let (da, db) = (digits(self.coefficient), digits(other.coefficient));
            let (pa, pb) = (da as i64 + self.exponent, db as i64 + other.exponent);
            if pa != pb {
                pa.cmp(&pb)
            } else {
                // The same position: both coefficients with 34 digits.
                (self.coefficient * POW10[(DIGITS - da) as usize]).cmp(&(other.coefficient * POW10[(DIGITS - db) as usize]))
            }
        };
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    /// Without trailing zeros: the same value always gives the same parts.
    pub fn normalized(self) -> Decimal128 {
        if self.is_zero() {
            return ZERO;
        }
        let (mut coefficient, mut exponent) = (self.coefficient, self.exponent);
        for step in [16usize, 8, 4, 2, 1] {
            loop {
                let quotient = coefficient / POW10[step];
                if quotient * POW10[step] != coefficient {
                    break;
                }
                coefficient = quotient;
                exponent += step as i64;
            }
        }
        Decimal128 { negative: self.negative, coefficient, exponent }
    }

    pub fn hash(self) -> u64 {
        let d = self.normalized();
        let mixed = d.coefficient ^ ((d.exponent as u128) << 113) ^ ((d.negative as u128) << 127);
        (mixed as u64) ^ ((mixed >> 64) as u64).rotate_left(29)
    }

    /// The integer part by `mode`: 0 floor, 1 ceiling, 2 nearest (half away from zero), 3 toward
    /// zero. `None` when it does not fit in an int.
    pub fn to_int(self, mode: i32) -> Option<i64> {
        let (whole, rest, unit) = if self.exponent >= 0 {
            if self.is_zero() {
                return Some(0);
            }
            if self.exponent > 19 {
                return None;
            }
            (self.coefficient.checked_mul(POW10[self.exponent as usize])?, 0, 1)
        } else if self.exponent < -38 {
            (0, self.coefficient, u128::MAX)
        } else {
            let unit = POW10[(-self.exponent) as usize];
            (self.coefficient / unit, self.coefficient % unit, unit)
        };
        let away = rest != 0
            && match mode {
                0 => self.negative,
                1 => !self.negative,
                2 => unit != u128::MAX && rest >= unit - unit / 2,
                _ => false,
            };
        let magnitude = whole + away as u128;
        if self.negative {
            if magnitude > i64::MAX as u128 + 1 {
                return None;
            }
            Some((magnitude as i128).wrapping_neg() as i64)
        } else {
            i64::try_from(magnitude).ok()
        }
    }

    /// The shortest decimal that is the same float again (at most 17 digits): exact both ways.
    pub fn from_float(value: f64) -> Decimal128 {
        parse(&format!("{:e}", value)).unwrap_or(ZERO)
    }

    pub fn to_float(self) -> Option<f64> {
        let sign = if self.negative { "-" } else { "" };
        let value: f64 = format!("{}{}e{}", sign, self.coefficient, self.exponent).parse().ok()?;
        value.is_finite().then_some(value)
    }
}

/// `coefficient * 10^scale` in 256 bits (scale up to 76).
fn scaled(coefficient: u128, scale: u32) -> Wide {
    if scale <= 38 {
        return wide_mul(coefficient, POW10[scale as usize]);
    }
    let first = wide_mul(coefficient, POW10[38]);
    // The first product is below 10^72: times 10^(scale - 38) still fits.
    let low = narrow(&first);
    match low {
        Some(low) => wide_mul(low, POW10[(scale - 38) as usize]),
        None => {
            let high = first[2] as u128 | (first[3] as u128) << 64;
            let low = first[0] as u128 | (first[1] as u128) << 64;
            let power = POW10[(scale - 38) as usize];
            let a = wide_mul(low, power);
            let b = wide_mul(high, power);
            // a + b * 2^128
            let mut out = a;
            let (sum, carry) = out[2].overflowing_add(b[0]);
            out[2] = sum;
            out[3] = out[3].wrapping_add(b[1]).wrapping_add(carry as u64);
            out
        }
    }
}

fn wide_to_float(n: &Wide) -> f64 {
    n.iter().rev().fold(0.0, |total, &word| total * 18446744073709551616.0 + word as f64)
}

/// A product kept with 38 digits, for `power`.
#[derive(Clone, Copy)]
struct Kept {
    coefficient: u128,
    exponent: i64,
    sticky: bool,
}

impl Kept {
    fn multiply(self, other: Kept) -> Kept {
        let (coefficient, exponent, sticky) = cut(wide_mul(self.coefficient, other.coefficient), self.exponent + other.exponent, self.sticky || other.sticky);
        Kept { coefficient, exponent, sticky }
    }
}

/// A decimal from text like `-12.5e3`, rounded to 34 digits.
pub fn parse(text: &str) -> Result<Decimal128, crate::decimal::ParseFailure> {
    use crate::decimal::ParseFailure;
    let parts = crate::decimal::parse_parts(text)?;
    if parts.magnitude == 0 && !parts.sticky {
        return Ok(ZERO);
    }
    finish(parts.negative, parts.magnitude, parts.exponent, parts.sticky).map_err(|_| ParseFailure::OutOfRange)
}

impl std::fmt::Display for Decimal128 {
    /// Like a float: `1.5`, `2.0` for a whole number, `1.5e40` from 1e34 on.
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        let value = self.normalized();
        if value.is_zero() {
            return write!(formatter, "0.0");
        }
        let sign = if value.negative { "-" } else { "" };
        let digits = value.coefficient.to_string();
        let position = digits.len() as i64 + value.exponent;
        if position > DIGITS as i64 {
            let mantissa = if digits.len() > 1 { format!("{}.{}", &digits[..1], &digits[1..]) } else { digits };
            return write!(formatter, "{}{}e{}", sign, mantissa, position - 1);
        }
        if value.exponent >= 0 {
            return write!(formatter, "{}{}{}.0", sign, digits, "0".repeat(value.exponent as usize));
        }
        if position > 0 {
            let point = position as usize;
            return write!(formatter, "{}{}.{}", sign, &digits[..point], &digits[point..]);
        }
        write!(formatter, "{}0.{}{}", sign, "0".repeat((-position) as usize), digits)
    }
}

// The functions of compiled programs: a decimal is a `KojiDecimal128` in C.

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KojiDecimal128 {
    lo: u64,
    hi: u64,
}

fn from_c(value: KojiDecimal128) -> Decimal128 {
    Decimal128::from_bits(value.lo, value.hi)
}

fn to_c(value: Decimal128) -> KojiDecimal128 {
    let (lo, hi) = value.to_bits();
    KojiDecimal128 { lo, hi }
}

fn located(result: Result<Decimal128, Limit>, location: *const c_char) -> KojiDecimal128 {
    to_c(result.unwrap_or_else(|limit| panic_at(location, limit.message())))
}

fn divided<T>(result: Option<T>, location: *const c_char) -> T {
    result.unwrap_or_else(|| panic_at(location, "division by zero"))
}

#[no_mangle]
pub extern "C" fn koji_decimal128_add(a: KojiDecimal128, b: KojiDecimal128, location: *const c_char) -> KojiDecimal128 {
    located(from_c(a).add(from_c(b)), location)
}

#[no_mangle]
pub extern "C" fn koji_decimal128_sub(a: KojiDecimal128, b: KojiDecimal128, location: *const c_char) -> KojiDecimal128 {
    located(from_c(a).subtract(from_c(b)), location)
}

#[no_mangle]
pub extern "C" fn koji_decimal128_mul(a: KojiDecimal128, b: KojiDecimal128, location: *const c_char) -> KojiDecimal128 {
    located(from_c(a).multiply(from_c(b)), location)
}

#[no_mangle]
pub extern "C" fn koji_decimal128_div(a: KojiDecimal128, b: KojiDecimal128, location: *const c_char) -> KojiDecimal128 {
    located(divided(from_c(a).divide(from_c(b)), location), location)
}

#[no_mangle]
pub extern "C" fn koji_decimal128_rem(a: KojiDecimal128, b: KojiDecimal128, location: *const c_char) -> KojiDecimal128 {
    to_c(divided(from_c(a).remainder(from_c(b)), location))
}

#[no_mangle]
pub extern "C" fn koji_decimal128_compare(a: KojiDecimal128, b: KojiDecimal128) -> i32 {
    from_c(a).compare(from_c(b)) as i32
}

#[no_mangle]
pub extern "C" fn koji_decimal128_from_int(value: i64) -> KojiDecimal128 {
    to_c(Decimal128::from_int(value))
}

/// A constant: `coefficient * 10^exponent` (for the benchmark and the tests).
#[no_mangle]
pub extern "C" fn koji_decimal128_make(negative: bool, coefficient: u64, exponent: i32) -> KojiDecimal128 {
    to_c(Decimal128::new(negative, coefficient as u128, exponent as i64).unwrap_or(ZERO))
}

#[no_mangle]
pub extern "C" fn koji_string_from_decimal128(value: KojiDecimal128) -> *mut crate::string::KojiString {
    crate::string::new_string(from_c(value).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal128 {
        parse(text).unwrap()
    }

    fn show(result: Result<Decimal128, Limit>) -> String {
        match result {
            Ok(value) => value.to_string(),
            Err(limit) => limit.message().to_string(),
        }
    }

    #[test]
    fn counting_digits() {
        for (n, &power) in POW10.iter().enumerate() {
            assert_eq!(digits(power), n as u32 + 1);
            if n > 0 {
                assert_eq!(digits(power - 1), n as u32);
            }
        }
        assert_eq!(digits(u128::MAX), 39);
    }

    #[test]
    fn wide_arithmetic() {
        let a = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFFu128;
        let product = wide_mul(a, a);
        assert_eq!(product, [1, 0, u64::MAX - 1, u64::MAX]);
        let (quotient, rest) = wide_divide(&product, a);
        assert_eq!(narrow(&quotient), Some(a));
        assert_eq!(rest, 0);
        // n = x * d + r, for divisors of every size.
        let x = 3u128.pow(70);
        for d in [7u128, 1 << 40, 1_000_000_000_000_000_000_007, POW10[30] + 7, u128::MAX / 3] {
            let r = d / 2 + 1;
            let mut n = wide_mul(x, d);
            let (low, carry) = (n[0] as u128 | (n[1] as u128) << 64).overflowing_add(r);
            n[0] = low as u64;
            n[1] = (low >> 64) as u64;
            if carry {
                n[2] += 1;
            }
            let (quotient, rest) = wide_divide(&n, d);
            assert_eq!((narrow(&quotient), rest), (Some(x), r), "divisor {}", d);
        }
        let (quotient, rest) = wide_divide(&wide_mul(POW10[37], POW10[30]), POW10[33] + 1);
        // (10^33 + 1) * (10^34 - 10) = 10^67 - 10
        assert_eq!(narrow(&quotient), Some(POW10[34] - 10));
        assert_eq!(rest, 10);
    }

    #[test]
    fn divisions_by_one_word() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let n = (state as u128) << 64 | state.rotate_left(17) as u128;
            let n = n >> (state % 128);
            let d = (state.rotate_left(29) >> (state % 64)).max(1);
            assert_eq!(divide_small(n, d), (n / d as u128, (n % d as u128) as u64));
        }
    }

    /// quotient * divisor + rest == dividend, and rest < divisor, for numbers chosen to reach the
    /// corrections of the algorithm (divisors full of ones, dividends near their multiples).
    #[test]
    fn wide_division_property() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let shapes = |word: u64, kind: u64| match kind % 4 {
            0 => word,
            1 => u64::MAX,
            2 => 1 << (word % 64),
            _ => u64::MAX - (word % 3),
        };
        for _ in 0..200_000 {
            let d = (shapes(next(), next()) as u128) << 64 | shapes(next(), next()) as u128;
            let d = if d == 0 { 1 } else { d >> (next() % 128) }.max(1);
            let n = [shapes(next(), next()), shapes(next(), next()), shapes(next(), next()), shapes(next(), next()) >> (next() % 64)];
            let (q, r) = wide_divide(&n, d);
            assert!(r < d);
            // q * d + r, with q of up to 256 bits
            let mut total = [0u64; 4];
            let mut carry = 0u128;
            for (i, part) in [wide_mul(q[0] as u128 | (q[1] as u128) << 64, d), {
                let high = wide_mul(q[2] as u128 | (q[3] as u128) << 64, d);
                [0, 0, high[0], high[1]]
            }]
            .iter()
            .enumerate()
            {
                let _ = i;
                let mut c = 0u128;
                for w in 0..4 {
                    let sum = total[w] as u128 + part[w] as u128 + c;
                    total[w] = sum as u64;
                    c = sum >> 64;
                }
                carry += c;
            }
            let mut c = r;
            for word in total.iter_mut() {
                let sum = *word as u128 + (c as u64) as u128;
                *word = sum as u64;
                c = (c >> 64) + (sum >> 64);
            }
            assert_eq!(total, n, "{:?} / {}", n, d);
            let _ = carry;
        }
    }

    #[test]
    fn exact_sums_and_rounding() {
        assert_eq!(show(d("0.1").add(d("0.2"))), "0.3");
        assert_eq!(show(d("1").divide(d("3")).unwrap()), "0.3333333333333333333333333333333333");
        assert_eq!(show(d("2").divide(d("3")).unwrap()), "0.6666666666666666666666666666666667");
        assert_eq!(show(d("1").divide(d("4")).unwrap()), "0.25");
        assert_eq!(show(d("1").divide(d("98765432109876543210")).unwrap()), "0.00000000000000000001012499999886093750011548828123843");
        assert_eq!(show(d("1").add(d("1e-40"))), "1.0");
        assert_eq!(show(d("1").subtract(d("1e-40"))), "1.0");
        assert_eq!(show(d("1e34").subtract(d("0.5"))), "1e34");
        assert_eq!(show(d("1e34").subtract(d("1.5"))), "9999999999999999999999999999999998.0");
        assert_eq!(show(d("1.50").multiply(d("2"))), "3.0");
        assert_eq!(show(d("12345678901234567890").multiply(d("98765432109876543210"))), "1.219326311370217952237463801111264e39");
        assert!(d("1.50").compare(d("1.5")).is_eq());
        assert_eq!(d("1.50").hash(), d("1.5").hash());
        assert_eq!(Decimal128::from_bits(0, 0), ZERO);
    }

    #[test]
    fn limits() {
        assert_eq!(show(d("9999999999999999999999999999999999e6111").add(d("1e6111"))), "decimal overflow");
        assert_eq!(show(d("1e-6176").divide(d("10")).unwrap()), "decimal underflow");
        assert_eq!(d("3e-6176").divide(d("2")).unwrap().unwrap().compare(d("2e-6176")), Ordering::Equal);
        assert_eq!(d("1e6144").to_string(), "1e6144");
        assert!(parse("1e6145").is_err());
    }

    #[test]
    fn floats_go_and_come_back() {
        let z = 0.1 + 0.2;
        assert_eq!(Decimal128::from_float(z).to_string(), "0.30000000000000004");
        assert_eq!(Decimal128::from_float(z).to_float(), Some(z));
        assert_eq!(d("0.3").to_float(), Some(0.3));
        for value in [1e-300, 123.456, 6.02214076e23, f64::MAX, f64::MIN_POSITIVE, 5e-324] {
            assert_eq!(Decimal128::from_float(value).to_float(), Some(value));
        }
    }

    #[test]
    fn powers_and_roots() {
        assert_eq!(show(d("1.1").power(2).unwrap()), "1.21");
        assert_eq!(show(d("2").power(-2).unwrap()), "0.25");
        assert_eq!(show(d("2").power(128).unwrap()), "3.402823669209384634633746074317682e38");
        assert_eq!(d("0.0625").sqrt().unwrap().to_string(), "0.25");
        assert_eq!(d("2").sqrt().unwrap().to_string(), "1.414213562373095048801688724209698");
    }

    #[test]
    fn remainders() {
        assert_eq!(d("7.5").remainder(d("2")).unwrap().to_string(), "1.5");
        assert_eq!(d("-7.5").remainder(d("2")).unwrap().to_string(), "-1.5");
        assert_eq!(d("1e300").remainder(d("7")).unwrap().to_string(), "1.0");
        assert_eq!(show(d("1.1").power(2).unwrap()), "1.21");
        assert_eq!(show(d("2").power(-2).unwrap()), "0.25");
        assert_eq!(show(d("2").power(128).unwrap()), "3.402823669209384634633746074317682e38");
        assert_eq!(d("2").sqrt().unwrap().to_string(), "1.414213562373095048801688724209698");
        assert_eq!(d("0.0625").sqrt().unwrap().to_string(), "0.25");
        assert_eq!(d("2.5").to_int(2), Some(3));
        assert_eq!(d("-2.5").to_int(0), Some(-3));
        assert_eq!(d("-2.7").to_int(3), Some(-2));
    }
}

/// The operations against Python's `decimal` with the same context (34 digits, half to even):
/// `KOJI_DECIMAL128_CASES=<file> cargo test -- --ignored` with lines `op a b result`.
#[cfg(test)]
mod reference {
    use super::*;

    #[test]
    #[ignore]
    fn against_python() {
        let Ok(path) = std::env::var("KOJI_DECIMAL128_CASES") else { return };
        let text = std::fs::read_to_string(path).unwrap();
        let mut failures = 0;
        for line in text.lines() {
            let parts: Vec<&str> = line.split(' ').collect();
            let (a, b) = (parse(parts[1]).unwrap(), parse(parts[2]).unwrap());
            let expected = parse(parts[3]).unwrap();
            let found = match parts[0] {
                "add" => a.add(b).ok(),
                "sub" => a.subtract(b).ok(),
                "mul" => a.multiply(b).ok(),
                "div" => a.divide(b).and_then(Result::ok),
                "rem" => a.remainder(b),
                _ => a.sqrt(),
            };
            let same = found.is_some_and(|f| f.compare(expected).is_eq() && (f.negative == expected.negative || f.is_zero()));
            if !same {
                failures += 1;
                if failures < 10 {
                    println!("{} → {:?}, expected {}", line, found.map(|f| f.to_string()), expected);
                }
            }
        }
        assert_eq!(failures, 0, "{} of {} differ", failures, text.lines().count());
    }
}
