//! `decimal`: decimal floating point with 16 significant digits (the precision of IEEE 754
//! decimal64), computed in software. A literal like `0.1` is exactly one tenth; an operation
//! rounds its exact result to 16 digits, half to even.
//!
//! A value is kept normalized: its coefficient has no trailing zeros (`1.50` is `15e-1`), so equal
//! values have the same bits, and there is no negative zero. There is no NaN and no infinity: a
//! result too large panics (`decimal overflow`), and so does a result too small to be anything
//! but zero (`decimal underflow`).
//!
//! The 64 bits are `((exponent + 398) * 10^16 + coefficient) * 2 + negative`, with the
//! coefficient below 10^16 and the exponent from -398 to 384 (the largest value is
//! 9999999999999999e369, the smallest 1e-398). Zero is all zero bits.

use std::ffi::c_char;

use crate::output::panic_at;

const DIGITS: u32 = 16;
const TEN_16: u128 = 10_000_000_000_000_000;
const MIN_EXPONENT: i64 = -398;
/// Digits of the coefficient plus the exponent: a value is below 10^LIMIT.
const LIMIT: i64 = 385;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    pub negative: bool,
    pub coefficient: u64,
    pub exponent: i64,
}

/// Why a result has no `decimal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Overflow,
    Underflow,
}

impl Limit {
    pub fn message(self) -> &'static str {
        match self {
            Limit::Overflow => "decimal overflow",
            Limit::Underflow => "decimal underflow",
        }
    }
}

/// Zero has the smallest exponent: its bits are 0, the value of a slot filled with zeros.
const ZERO: Decimal = Decimal { negative: false, coefficient: 0, exponent: MIN_EXPONENT };

fn pow10(n: u32) -> u128 {
    10u128.pow(n)
}

fn digits(n: u128) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// Rounds `magnitude * 10^exponent`, plus a positive amount smaller than one unit of its last
/// digit when `sticky`, to a `decimal`.
fn round(negative: bool, magnitude: u128, exponent: i64, sticky: bool) -> Result<Decimal, Limit> {
    if magnitude == 0 && !sticky {
        return Ok(ZERO);
    }
    let count = digits(magnitude) as i64;
    let drop = (count - DIGITS as i64).max(MIN_EXPONENT - exponent).max(0);
    let (mut kept, mut exponent) = (magnitude, exponent);
    if drop > 0 {
        let (quotient, up) = if drop > 38 {
            (0, false)
        } else {
            let unit = pow10(drop as u32);
            let (quotient, rest) = (magnitude / unit, magnitude % unit);
            let half = unit / 2;
            let up = rest > half || (rest == half && (sticky || quotient % 2 == 1));
            (quotient, up)
        };
        kept = quotient + up as u128;
        exponent += drop;
        if kept == TEN_16 {
            kept /= 10;
            exponent += 1;
        }
    }
    if kept == 0 {
        return Err(Limit::Underflow);
    }
    while kept % 10 == 0 {
        kept /= 10;
        exponent += 1;
    }
    if digits(kept) as i64 + exponent > LIMIT {
        return Err(Limit::Overflow);
    }
    Ok(Decimal { negative, coefficient: kept as u64, exponent })
}

impl Decimal {
    pub fn from_bits(bits: u64) -> Decimal {
        let negative = bits & 1 == 1;
        let rest = bits >> 1;
        let coefficient = rest % TEN_16 as u64;
        let exponent = (rest / TEN_16 as u64) as i64 + MIN_EXPONENT;
        Decimal { negative, coefficient, exponent }
    }

    pub fn to_bits(self) -> u64 {
        ((((self.exponent - MIN_EXPONENT) as u64) * TEN_16 as u64 + self.coefficient) << 1) | self.negative as u64
    }

    pub fn is_zero(self) -> bool {
        self.coefficient == 0
    }

    pub fn from_int(value: i64) -> Decimal {
        round(value < 0, value.unsigned_abs() as u128, 0, false).unwrap_or(ZERO)
    }

    pub fn negate(self) -> Decimal {
        if self.is_zero() {
            self
        } else {
            Decimal { negative: !self.negative, ..self }
        }
    }

    pub fn abs(self) -> Decimal {
        Decimal { negative: false, ..self }
    }

    pub fn add(self, other: Decimal) -> Result<Decimal, Limit> {
        if self.is_zero() {
            return Ok(other);
        }
        if other.is_zero() {
            return Ok(self);
        }
        // `a` has the larger exponent: it is shifted left, `b` is cut when it is far below.
        let (a, b) = if self.exponent >= other.exponent { (self, other) } else { (other, self) };
        let distance = (a.exponent - b.exponent) as u32;
        let (big, small, exponent, sticky) = if distance <= 22 {
            (a.coefficient as u128 * pow10(distance), b.coefficient as u128, b.exponent, false)
        } else {
            let shift = distance - 22;
            let (small, sticky) = if shift > 38 {
                (0, true)
            } else {
                let unit = pow10(shift);
                (b.coefficient as u128 / unit, !(b.coefficient as u128).is_multiple_of(unit))
            };
            (a.coefficient as u128 * pow10(22), small, a.exponent - 22, sticky)
        };
        if a.negative == b.negative {
            return round(a.negative, big + small, exponent, sticky);
        }
        if big >= small {
            // When `small` was cut, the exact difference is a little less than `big - small`.
            let difference = big - small - sticky as u128;
            round(a.negative, difference, exponent, sticky)
        } else {
            round(b.negative, small - big, exponent, false)
        }
    }

    pub fn subtract(self, other: Decimal) -> Result<Decimal, Limit> {
        self.add(other.negate())
    }

    pub fn multiply(self, other: Decimal) -> Result<Decimal, Limit> {
        if self.is_zero() || other.is_zero() {
            return Ok(ZERO);
        }
        round(
            self.negative != other.negative,
            self.coefficient as u128 * other.coefficient as u128,
            self.exponent + other.exponent,
            false,
        )
    }

    /// `None` when `other` is zero.
    pub fn divide(self, other: Decimal) -> Option<Result<Decimal, Limit>> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(Ok(ZERO));
        }
        Some(long_division(
            self.negative != other.negative,
            self.coefficient as u128,
            other.coefficient as u128,
            self.exponent - other.exponent,
        ))
    }

    /// The remainder of the division truncated toward zero: it has the sign of `self`, like the
    /// remainder of ints. Always exact. `None` when `other` is zero.
    pub fn remainder(self, other: Decimal) -> Option<Decimal> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(ZERO);
        }
        let (a, b) = (self.coefficient as u128, other.coefficient as u128);
        let (rest, exponent) = if self.exponent >= other.exponent {
            // (a * 10^d) mod b, without computing 10^d.
            let mut power = 1 % b;
            let mut base = 10 % b;
            let mut d = (self.exponent - other.exponent) as u64;
            while d > 0 {
                if d & 1 == 1 {
                    power = power * base % b;
                }
                base = base * base % b;
                d >>= 1;
            }
            (a % b * power % b, other.exponent)
        } else {
            let d = (other.exponent - self.exponent) as u32;
            let rest = if d >= 17 { a } else { a % (b * pow10(d)) };
            (rest, self.exponent)
        };
        // Exact: the remainder is smaller than both operands.
        Some(round(self.negative, rest, exponent, false).unwrap_or(ZERO))
    }

    /// `self ** exponent`; `None` for zero to a negative power.
    pub fn power(self, exponent: i64) -> Option<Result<Decimal, Limit>> {
        if exponent == 0 {
            return Some(Ok(Decimal::from_int(1)));
        }
        if self.is_zero() {
            return if exponent < 0 { None } else { Some(Ok(ZERO)) };
        }
        // Squaring with 19 digits (three more than the result keeps), then one rounding.
        let negative = self.negative && exponent % 2 != 0;
        let mut result = Wide { coefficient: 1, exponent: 0, sticky: false };
        let mut base = Wide { coefficient: self.coefficient as u128, exponent: self.exponent, sticky: false };
        let mut n = exponent.unsigned_abs();
        while n > 0 {
            if n & 1 == 1 {
                result = result.multiply(base);
            }
            n >>= 1;
            if n > 0 {
                base = base.multiply(base);
            }
            let position = |wide: &Wide| digits(wide.coefficient) as i64 + wide.exponent;
            if position(&result) > 2 * LIMIT || position(&base) > 2 * LIMIT {
                return Some(Err(if exponent > 0 { Limit::Overflow } else { Limit::Underflow }));
            }
            if position(&result) < 2 * MIN_EXPONENT || position(&base) < 2 * MIN_EXPONENT {
                return Some(Err(if exponent > 0 { Limit::Underflow } else { Limit::Overflow }));
            }
        }
        if exponent > 0 {
            return Some(round(negative, result.coefficient, result.exponent, result.sticky));
        }
        Some(long_division(negative, 1, result.coefficient, -result.exponent))
    }

    /// The square root, correctly rounded; `None` for a negative number.
    pub fn sqrt(self) -> Option<Decimal> {
        if self.negative {
            return None;
        }
        if self.is_zero() {
            return Some(ZERO);
        }
        // The coefficient with 33 or 34 digits and an even exponent: its root has 17 digits.
        let mut shift = 34 - digits(self.coefficient as u128) as i64;
        if (self.exponent - shift) % 2 != 0 {
            shift -= 1;
        }
        let scaled = self.coefficient as u128 * pow10(shift as u32);
        let root = scaled.isqrt();
        let sticky = root * root != scaled;
        // A root is between 1e-199 and 1e193: it always fits.
        Some(round(false, root, (self.exponent - shift) / 2, sticky).unwrap_or(ZERO))
    }

    pub fn compare(self, other: Decimal) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self.is_zero(), other.is_zero()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return if other.negative { Ordering::Greater } else { Ordering::Less },
            (false, true) => return if self.negative { Ordering::Less } else { Ordering::Greater },
            _ => {}
        }
        if self.negative != other.negative {
            return if self.negative { Ordering::Less } else { Ordering::Greater };
        }
        let magnitude = {
            let position = |d: Decimal| digits(d.coefficient as u128) as i64 + d.exponent;
            let (a, b) = (position(self), position(other));
            if a != b {
                a.cmp(&b)
            } else {
                // The same position: the coefficients with 16 digits each.
                let scale = |d: Decimal| d.coefficient as u128 * pow10(DIGITS - digits(d.coefficient as u128));
                scale(self).cmp(&scale(other))
            }
        };
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    /// The integer part, rounded by `mode`: 0 floor, 1 ceiling, 2 to the nearest (half away from
    /// zero) and 3 toward zero. `None` when it does not fit in an int.
    pub fn to_int(self, mode: i32) -> Option<i64> {
        let coefficient = self.coefficient as u128;
        let (whole, fraction) = if self.exponent >= 0 {
            if self.exponent > 19 {
                return if self.is_zero() { Some(0) } else { None };
            }
            (coefficient.checked_mul(pow10(self.exponent as u32))?, Fraction::None)
        } else {
            let places = (-self.exponent) as u32;
            if places > 19 {
                (0, if coefficient == 0 { Fraction::None } else { Fraction::BelowHalf })
            } else {
                let unit = pow10(places);
                let rest = coefficient % unit;
                let fraction = match (rest * 2).cmp(&unit) {
                    _ if rest == 0 => Fraction::None,
                    std::cmp::Ordering::Less => Fraction::BelowHalf,
                    _ => Fraction::HalfOrMore,
                };
                (coefficient / unit, fraction)
            }
        };
        let away = match (mode, fraction) {
            (_, Fraction::None) => false,
            (0, _) => self.negative,
            (1, _) => !self.negative,
            (2, Fraction::HalfOrMore) => true,
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

    /// The nearest decimal to a float (every float is in the range of a decimal).
    pub fn from_float(value: f64) -> Decimal {
        // Rust writes the 16 digits of a float correctly rounded.
        parse(&format!("{:.15e}", value)).unwrap_or(ZERO)
    }

    /// The nearest float; `None` when it is too large for a float.
    pub fn to_float(self) -> Option<f64> {
        let sign = if self.negative { "-" } else { "" };
        let value: f64 = format!("{}{}e{}", sign, self.coefficient, self.exponent).parse().ok()?;
        if value.is_finite() {
            Some(value)
        } else {
            None
        }
    }
}

enum Fraction {
    None,
    BelowHalf,
    HalfOrMore,
}

/// A product kept with 19 digits, for `power`.
#[derive(Clone, Copy)]
struct Wide {
    coefficient: u128,
    exponent: i64,
    sticky: bool,
}

impl Wide {
    fn multiply(self, other: Wide) -> Wide {
        let mut coefficient = self.coefficient * other.coefficient;
        let mut exponent = self.exponent + other.exponent;
        let mut sticky = self.sticky || other.sticky;
        let count = digits(coefficient);
        if count > 19 {
            let unit = pow10(count - 19);
            sticky |= !coefficient.is_multiple_of(unit);
            coefficient /= unit;
            exponent += (count - 19) as i64;
        }
        Wide { coefficient, exponent, sticky }
    }
}

/// `dividend / divisor * 10^exponent`, with 18 digits of quotient and the rest as sticky.
fn long_division(negative: bool, dividend: u128, divisor: u128, exponent: i64) -> Result<Decimal, Limit> {
    let mut quotient = dividend / divisor;
    let mut rest = dividend % divisor;
    let mut exponent = exponent;
    while rest != 0 && quotient < pow10(17) {
        rest *= 10;
        quotient = quotient * 10 + rest / divisor;
        rest %= divisor;
        exponent -= 1;
    }
    round(negative, quotient, exponent, rest != 0)
}

/// Why a text is not a decimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseFailure {
    Invalid,
    OutOfRange,
}

/// A decimal from text like `-12.5e3` (surrounding spaces allowed), rounded to 16 digits.
pub fn parse(text: &str) -> Result<Decimal, ParseFailure> {
    let text = text.trim();
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (number, power) = match rest.find(['e', 'E']) {
        Some(at) => (&rest[..at], Some(&rest[at + 1..])),
        None => (rest, None),
    };
    let (whole, fraction) = match number.find('.') {
        Some(at) => (&number[..at], &number[at + 1..]),
        None => (number, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !all_digits(whole) || !all_digits(fraction) {
        return Err(ParseFailure::Invalid);
    }
    let mut exponent: i64 = match power {
        None => 0,
        Some(power) => {
            let unsigned = power.strip_prefix(['+', '-']).unwrap_or(power);
            if unsigned.is_empty() || !all_digits(unsigned) {
                return Err(ParseFailure::Invalid);
            }
            // A huge exponent is out of range anyway.
            let value: i64 = unsigned.get(..unsigned.len().min(6)).unwrap_or("0").parse().unwrap_or(0);
            let value = if unsigned.len() > 6 { 1_000_000 } else { value };
            if power.starts_with('-') {
                -value
            } else {
                value
            }
        }
    };
    // The significant digits (without leading zeros), the first 38 exactly and the rest as sticky.
    let mut magnitude: u128 = 0;
    let mut count = 0;
    let mut sticky = false;
    let mut dropped: i64 = 0;
    for (index, byte) in whole.bytes().chain(fraction.bytes()).enumerate() {
        let digit = (byte - b'0') as u128;
        let in_fraction = index >= whole.len();
        if magnitude == 0 && digit == 0 {
            if in_fraction {
                exponent -= 1;
            }
            continue;
        }
        if count < 38 {
            magnitude = magnitude * 10 + digit;
            count += 1;
            if in_fraction {
                exponent -= 1;
            }
        } else {
            sticky |= digit != 0;
            if !in_fraction {
                dropped += 1;
            }
        }
    }
    round(negative && (magnitude != 0 || sticky), magnitude, exponent + dropped, sticky).map_err(|_| ParseFailure::OutOfRange)
}

impl std::fmt::Display for Decimal {
    /// Like a float: `1.5`, `2.0` for a whole number, `1.5e20` from 1e16 on.
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_zero() {
            return write!(formatter, "0.0");
        }
        let sign = if self.negative { "-" } else { "" };
        let digits = self.coefficient.to_string();
        let position = digits.len() as i64 + self.exponent;
        if position > 16 {
            let mantissa = if digits.len() > 1 { format!("{}.{}", &digits[..1], &digits[1..]) } else { digits };
            return write!(formatter, "{}{}e{}", sign, mantissa, position - 1);
        }
        if self.exponent >= 0 {
            return write!(formatter, "{}{}{}.0", sign, digits, "0".repeat(self.exponent as usize));
        }
        if position > 0 {
            let point = position as usize;
            return write!(formatter, "{}{}.{}", sign, &digits[..point], &digits[point..]);
        }
        write!(formatter, "{}0.{}{}", sign, "0".repeat((-position) as usize), digits)
    }
}

// The functions of compiled programs. A decimal is a `uint64_t` in C.

fn located<T>(result: Result<T, Limit>, location: *const c_char) -> T {
    result.unwrap_or_else(|limit| panic_at(location, limit.message()))
}

fn divided<T>(result: Option<T>, location: *const c_char) -> T {
    result.unwrap_or_else(|| panic_at(location, "division by zero"))
}

#[no_mangle]
pub extern "C" fn koji_decimal_add(a: u64, b: u64, location: *const c_char) -> u64 {
    located(Decimal::from_bits(a).add(Decimal::from_bits(b)), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_sub(a: u64, b: u64, location: *const c_char) -> u64 {
    located(Decimal::from_bits(a).subtract(Decimal::from_bits(b)), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_mul(a: u64, b: u64, location: *const c_char) -> u64 {
    located(Decimal::from_bits(a).multiply(Decimal::from_bits(b)), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_div(a: u64, b: u64, location: *const c_char) -> u64 {
    located(divided(Decimal::from_bits(a).divide(Decimal::from_bits(b)), location), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_rem(a: u64, b: u64, location: *const c_char) -> u64 {
    divided(Decimal::from_bits(a).remainder(Decimal::from_bits(b)), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_pow(base: u64, exponent: i64, location: *const c_char) -> u64 {
    located(divided(Decimal::from_bits(base).power(exponent), location), location).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_neg(value: u64) -> u64 {
    Decimal::from_bits(value).negate().to_bits()
}

/// -1, 0 or 1.
#[no_mangle]
pub extern "C" fn koji_decimal_compare(a: u64, b: u64) -> i32 {
    Decimal::from_bits(a).compare(Decimal::from_bits(b)) as i32
}

#[no_mangle]
pub extern "C" fn koji_decimal_from_int(value: i64) -> u64 {
    Decimal::from_int(value).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_from_float(value: f64) -> u64 {
    Decimal::from_float(value).to_bits()
}

#[no_mangle]
pub extern "C" fn koji_decimal_to_float(value: u64, location: *const c_char) -> f64 {
    Decimal::from_bits(value).to_float().unwrap_or_else(|| panic_at(location, "float overflow"))
}

/// `int(d)` (mode 3, toward zero), `math.floor` (0), `math.ceil` (1) and `math.round` (2).
#[no_mangle]
pub extern "C" fn koji_decimal_to_int(value: u64, mode: i32, location: *const c_char) -> i64 {
    let decimal = Decimal::from_bits(value);
    decimal
        .to_int(mode)
        .unwrap_or_else(|| panic_at(location, &format!("integer overflow: {} does not fit in an int", decimal)))
}

#[no_mangle]
pub extern "C" fn koji_string_from_decimal(value: u64) -> *mut crate::string::KojiString {
    crate::string::new_string(Decimal::from_bits(value).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        parse(text).unwrap()
    }

    fn show(result: Result<Decimal, Limit>) -> String {
        match result {
            Ok(value) => value.to_string(),
            Err(limit) => limit.message().to_string(),
        }
    }

    #[test]
    fn exact_sums_and_rounding() {
        assert_eq!(show(d("0.1").add(d("0.2"))), "0.3");
        assert_eq!(show(d("1").divide(d("3")).unwrap()), "0.3333333333333333");
        assert_eq!(show(d("2").divide(d("3")).unwrap()), "0.6666666666666667");
        assert_eq!(show(d("1").add(d("1e-20"))), "1.0");
        assert_eq!(show(d("1").subtract(d("1e-20"))), "1.0");
        assert_eq!(show(d("1e16").subtract(d("0.5"))), "1e16");
        assert_eq!(show(d("1e16").subtract(d("1.5"))), "9999999999999998.0");
        assert_eq!(show(d("0.5").add(d("9999999999999999"))), "1e16");
        assert_eq!(show(d("1.5").add(d("9999999999999998"))), "1e16");
        assert_eq!(show(d("1.50").multiply(d("2"))), "3.0");
        assert_eq!(show(d("-7.5").subtract(d("2.5"))), "-10.0");
        assert_eq!(d("1.50").to_bits(), d("1.5").to_bits());
        assert_eq!(d("-0").to_bits(), d("0").to_bits());
    }

    #[test]
    fn limits() {
        assert_eq!(show(d("9999999999999999e369").add(d("1e369"))), "decimal overflow");
        assert_eq!(show(d("1e-398").divide(d("10")).unwrap()), "decimal underflow");
        assert_eq!(show(d("1e-398").divide(d("2")).unwrap()), "decimal underflow");
        assert_eq!(d("3e-398").divide(d("2")).unwrap(), Ok(d("2e-398")));
        assert_eq!(d("1e-397").divide(d("4")).unwrap(), Ok(d("2e-398")));
        assert_eq!(d("1e-395").multiply(d("1.234567")), Ok(d("1.235e-395")));
        assert_eq!(parse("1e400"), Err(ParseFailure::OutOfRange));
        assert_eq!(parse("1.2.3"), Err(ParseFailure::Invalid));
        assert_eq!(parse("e5"), Err(ParseFailure::Invalid));
        assert_eq!(d("12345678901234567").to_string(), "1.234567890123457e16");
        assert_eq!(d("0.000000000000000000000000000000000000000012345").to_string(), "0.000000000000000000000000000000000000000012345");
    }

    #[test]
    fn remainders_powers_and_roots() {
        assert_eq!(d("7.5").remainder(d("2")).unwrap().to_string(), "1.5");
        assert_eq!(d("-7.5").remainder(d("2")).unwrap().to_string(), "-1.5");
        assert_eq!(d("1e300").remainder(d("7")).unwrap().to_string(), "1.0");
        assert_eq!(d("0.3").remainder(d("1e20")).unwrap().to_string(), "0.3");
        assert_eq!(show(d("1.1").power(2).unwrap()), "1.21");
        assert_eq!(show(d("2").power(-2).unwrap()), "0.25");
        assert_eq!(show(d("10").power(400).unwrap()), "decimal overflow");
        assert_eq!(show(d("2").power(64).unwrap()), "1.844674407370955e19");
        assert!(d("0").power(-1).is_none());
        assert_eq!(d("2").sqrt().unwrap().to_string(), "1.414213562373095");
        assert_eq!(d("0.0625").sqrt().unwrap().to_string(), "0.25");
        assert_eq!(d("1e-397").sqrt(), Some(d("3.162277660168379e-199")));
    }

    #[test]
    fn order_conversions_and_text() {
        assert!(d("0.1").compare(d("0.09")).is_gt());
        assert!(d("-1").compare(d("0.5")).is_lt());
        assert!(d("-2").compare(d("-1")).is_lt());
        assert!(d("1e20").compare(d("9999999999999999e4")).is_gt());
        assert!(d("1e20").compare(d("99999999999999999999")).is_eq());
        assert_eq!(d("2.5").to_int(2), Some(3));
        assert_eq!(d("-2.5").to_int(2), Some(-3));
        assert_eq!(d("-2.5").to_int(0), Some(-3));
        assert_eq!(d("-2.5").to_int(3), Some(-2));
        assert_eq!(d("2.1").to_int(1), Some(3));
        assert_eq!(d("1e19").to_int(3), None);
        assert_eq!(d("-9223372036854775808").to_int(3), None);
        assert_eq!(d("-922337203685477").to_int(3), Some(-922337203685477));
        assert_eq!(Decimal::from_float(0.1).to_string(), "0.1");
        assert_eq!(Decimal::from_float(0.1 + 0.2).to_string(), "0.3");
        assert_eq!(d("0.1").to_float(), Some(0.1));
        assert_eq!(d("1e309").to_float(), None);
        assert_eq!(Decimal::from_int(i64::MIN).to_string(), "-9.223372036854776e18");
        assert_eq!(d("1234.5").to_string(), "1234.5");
        assert_eq!(d("0.001").to_string(), "0.001");
        assert_eq!(d("100").to_string(), "100.0");
        assert_eq!(d("1e16").to_string(), "1e16");
        assert_eq!(d("-1.5e17").to_string(), "-1.5e17");
        assert_eq!(d("0").to_bits(), 0);
        assert_eq!(d("0.000").to_string(), "0.0");
        for text in ["0", "1", "-3.25", "1e-398", "9999999999999999e369", "0.1"] {
            assert_eq!(Decimal::from_bits(d(text).to_bits()), d(text));
        }
    }
}
