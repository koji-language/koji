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

/// A float written in hexadecimal (`0x1.8p1`, `-0xff`, `0x1p-1074`), rounded to the nearest
/// float when it has more digits than a float holds.
pub fn parse_hex_float(text: &str) -> Option<f64> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let rest = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X"))?;
    let (number, power) = match rest.find(['p', 'P']) {
        Some(at) => (&rest[..at], rest[at + 1..].parse::<i64>().ok()?),
        None => (rest, 0),
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    // The first 30 hex digits exactly; the others only say whether something follows.
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
    let mantissa = mantissa | sticky as u128;
    // 2^exponent in two halves: each one stays a normal float.
    let exponent = exponent.clamp(-4000, 4000) as i32;
    let value = mantissa as f64 * 2f64.powi(exponent / 2) * 2f64.powi(exponent - exponent / 2);
    Some(if negative { -value } else { value })
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
