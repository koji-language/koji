//! Numbers: formatting and the operators that need more than a C expression.

use std::ffi::c_char;

use crate::output::panic_at;

/// The text of a float: always with a decimal point (`2.0`), so it never looks like an int, and
/// in scientific notation from 1e16 on.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_string()
    } else if value.abs() >= 1e16 {
        format!("{:e}", value)
    } else if value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        format!("{}", value)
    }
}

/// Why `int ** int` has no `int` result.
pub fn int_pow(base: i64, exponent: i64) -> Result<i64, &'static str> {
    if base == 0 && exponent < 0 {
        return Err("division by zero");
    }
    if exponent < 0 {
        return Err("negative exponent with int operands; use a float base");
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
