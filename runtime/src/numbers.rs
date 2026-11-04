//! Numbers: formatting and the operators that need more than a C expression.

use std::ffi::c_char;

use crate::output::panic_at;

/// The text of a float, as it is stored: in hexadecimal, `0x1.8p1` for 3.0 (a mantissa in base
/// 16 and a power of 2), which is also how a float is written in code. Exact, and never mistaken
/// for a decimal.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let bits = value.to_bits();
    let sign = if bits >> 63 == 1 { "-" } else { "" };
    let stored_exponent = ((bits >> 52) & 0x7FF) as i64;
    let fraction = bits & ((1 << 52) - 1);
    if stored_exponent == 0 && fraction == 0 {
        return format!("{}0x0p0", sign);
    }
    // A subnormal number has no leading 1: 0x0.0000000000001p-1022.
    let (lead, exponent) = if stored_exponent == 0 { (0, -1022) } else { (1, stored_exponent - 1023) };
    let digits = format!("{:013x}", fraction);
    let digits = digits.trim_end_matches('0');
    if digits.is_empty() {
        format!("{}0x{}p{}", sign, lead, exponent)
    } else {
        format!("{}0x{}.{}p{}", sign, lead, digits, exponent)
    }
}

/// A float written in hexadecimal (`0x1.8p1`, `-0xff`, `0x1p-1074`), rounded to the nearest float
/// (half to even) when it has more digits than a float holds. `None` for a text that is not one;
/// an infinity for one too large.
pub fn parse_hex_float(text: &str) -> Option<f64> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let rest = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X"))?;
    let (number, power) = match rest.find(['p', 'P']) {
        Some(at) => (&rest[..at], rest[at + 1..].parse::<i64>().ok()?.clamp(-100_000, 100_000)),
        None => (rest, 0),
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    // value = mantissa * 2^exponent (+ a nonzero part below the mantissa's last bit when sticky);
    // the first 30 significant hex digits are kept exactly.
    let mut mantissa: u128 = 0;
    let mut kept = 0;
    let mut sticky = false;
    let mut exponent = power;
    for (index, c) in whole.chars().chain(fraction.chars()).enumerate() {
        let digit = c.to_digit(16)? as u128;
        let in_fraction = index >= whole.len();
        if kept < 30 {
            if mantissa != 0 || digit != 0 {
                kept += 1;
            }
            mantissa = mantissa << 4 | digit;
            if in_fraction {
                exponent -= 4;
            }
        } else {
            sticky |= digit != 0;
            if !in_fraction {
                exponent += 4;
            }
        }
    }
    let sign = if negative { -1.0 } else { 1.0 };
    if mantissa == 0 {
        return Some(sign * 0.0);
    }
    // The exponent of the leading bit, and the bits a float keeps: 53, fewer when subnormal.
    let bits = 128 - mantissa.leading_zeros() as i64;
    let leading = exponent + bits - 1;
    if leading > 1023 {
        return Some(sign * f64::INFINITY);
    }
    let precision = if leading < -1022 { 53 - (-1022 - leading) } else { 53 };
    let shift = bits - precision;
    let (mut kept_bits, mut scale) = (mantissa, exponent);
    if shift > 0 {
        let dropped = if shift >= 128 { mantissa } else { mantissa & ((1u128 << shift) - 1) };
        let half = if shift > 128 { 0 } else { 1u128 << (shift - 1) };
        kept_bits = if shift >= 128 { 0 } else { mantissa >> shift };
        let up = dropped > half || (dropped == half && (sticky || kept_bits & 1 == 1));
        kept_bits += up as u128;
        scale += shift;
    }
    // kept_bits * 2^scale is a float (or 2^1024 after rounding up): exact in two halves.
    let value = kept_bits as f64 * 2f64.powi((scale / 2) as i32) * 2f64.powi((scale - scale / 2) as i32);
    Some(sign * value)
}

/// Why `int ** int` has no `int` result.
pub fn int_pow(base: i64, exponent: i64) -> Result<i64, &'static str> {
    if base == 0 && exponent < 0 {
        return Err("division by zero");
    }
    if exponent < 0 {
        return Err("negative exponent with int operands; use a decimal base");
    }
    let exponent = u32::try_from(exponent).map_err(|_| "integer overflow")?;
    base.checked_pow(exponent).ok_or("integer overflow")
}

/// Why `float ** float` has no real result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowFailure {
    /// A negative base with a fractional exponent.
    Complex,
    /// `0 ** -1` is `1 / 0`.
    Infinite,
    /// The result does not fit in a float.
    Overflow,
}

/// `base ** exponent` with real numbers only (`0 ** 0` is 1).
pub fn float_pow(base: f64, exponent: f64) -> Result<f64, PowFailure> {
    if base < 0.0 && exponent.fract() != 0.0 {
        return Err(PowFailure::Complex);
    }
    if base == 0.0 && exponent < 0.0 {
        return Err(PowFailure::Infinite);
    }
    let result = base.powf(exponent);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(PowFailure::Overflow)
    }
}

/// `base ** exponent` for ints; panics at `location` when there is no int result.
#[no_mangle]
pub extern "C" fn koji_int_pow(base: i64, exponent: i64, location: *const c_char) -> i64 {
    match int_pow(base, exponent) {
        Ok(result) => result,
        Err(message) => panic_at(location, message),
    }
}

/// `base ** exponent` for floats; panics at `location` when there is no real result.
#[no_mangle]
pub extern "C" fn koji_float_pow(base: f64, exponent: f64, location: *const c_char) -> f64 {
    match float_pow(base, exponent) {
        Ok(result) => result,
        Err(PowFailure::Complex) => panic_at(
            location,
            &format!(
                "{} ** {} is not a real number (use math.pow to handle it)",
                format_float(base),
                format_float(exponent)
            ),
        ),
        Err(PowFailure::Infinite) => panic_at(location, "division by zero"),
        Err(PowFailure::Overflow) => panic_at(location, "float overflow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_in_hexadecimal() {
        for value in [0.1, 3.0, -0.5, 1e300, 5e-324, 2.2250738585072014e-308, f64::MAX, 123.456] {
            assert_eq!(parse_hex_float(&format_float(value)), Some(value), "{}", format_float(value));
        }
        assert_eq!(format_float(3.0), "0x1.8p1");
        assert_eq!(format_float(0.1), "0x1.999999999999ap-4");
        assert_eq!(format_float(5e-324), "0x0.0000000000001p-1022");
        // Rounded to the nearest, half to even.
        assert_eq!(parse_hex_float("0x1.00000000000008p0"), Some(1.0));
        assert_eq!(parse_hex_float("0x1.00000000000018p0"), Some(1.0 + 2.0 * f64::EPSILON));
        assert_eq!(parse_hex_float("0x1.000000000000081p0"), Some(1.0 + f64::EPSILON));
        assert_eq!(parse_hex_float("0x1.8p-1074"), Some(1e-323));
        assert_eq!(parse_hex_float("0x1p1024"), Some(f64::INFINITY));
        assert_eq!(parse_hex_float("0x"), None);
    }
}
