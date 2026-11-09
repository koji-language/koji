//! Numbers: floats written in hexadecimal, for `float(text)` (the operators and the text of a
//! float are in Koji: std/koji/numbers.kj).

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_in_hexadecimal() {
        assert_eq!(parse_hex_float("0x1.8p1"), Some(3.0));
        assert_eq!(parse_hex_float("0x1.999999999999ap-4"), Some(0.1));
        assert_eq!(parse_hex_float("0x0.0000000000001p-1022"), Some(5e-324));
        // Rounded to the nearest, half to even.
        assert_eq!(parse_hex_float("0x1.00000000000008p0"), Some(1.0));
        assert_eq!(parse_hex_float("0x1.00000000000018p0"), Some(1.0 + 2.0 * f64::EPSILON));
        assert_eq!(parse_hex_float("0x1.000000000000081p0"), Some(1.0 + f64::EPSILON));
        assert_eq!(parse_hex_float("0x1.8p-1074"), Some(1e-323));
        assert_eq!(parse_hex_float("0x1p1024"), Some(f64::INFINITY));
        assert_eq!(parse_hex_float("0x"), None);
    }
}
