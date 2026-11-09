//! The standard library for compiled programs: the built-in functions (`input`, `int`,
//! `float`) and the methods of strings that are not written in Koji.
//!
//! A function that gives a value of a declared type (`Option`, `Result`, an error) receives
//! its description from the generated code and builds it with `record::make`. The messages
//! of errors and the conversions of text come first.

use std::ffi::c_char;
use std::io::{BufRead, Write};

use crate::list::{new_list, KojiElementType, KojiList};
use crate::output::panic_at;
use crate::record::{field_record, make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

/// Called by `main` before anything else.
#[no_mangle]
pub extern "C" fn koji_init(argc: i32, argv: *const *const c_char) {
    crate::pool::adopt();
    crate::system::keep_arguments(argc, argv);
}

// The messages of errors and the conversions of text.

/// The readable message of an error of the standard library (`'x' is not a valid number`),
/// from the texts of its fields; `None` for any other type.
pub fn error_message(type_name: &str, variant: &str, texts: &[String]) -> Option<String> {
    const ERROR_TYPES: &[&str] = &[
        "ParseError",
        "HttpError",
    ];
    if !ERROR_TYPES.contains(&type_name) {
        return None;
    }
    let text = |i: usize| texts.get(i).cloned().unwrap_or_default();
    Some(match variant {
        "InvalidNumber" => format!("'{}' is not a valid number", text(0)),
        "OutOfRange" => format!("'{}' is out of range", text(0)),
        "InvalidUrl" => format!("invalid URL '{}'", text(0)),
        "ConnectionFailed" => format!("cannot connect to '{}': {}", text(0), text(1)),
        "Timeout" => format!("the request to '{}' timed out", text(0)),
        "Tls" => format!("secure connection to '{}' failed: {}", text(0), text(1)),
        "InvalidResponse" => format!("invalid response from '{}': {}", text(0), text(1)),
        _ => return None,
    })
}

/// `int('...')`: the number, or the variant of `ParseError` (`InvalidNumber`, `OutOfRange`).
pub fn parse_int(text: &str) -> Result<i64, &'static str> {
    let trimmed = text.trim();
    trimmed.parse::<i64>().map_err(|_| {
        let digits = trimmed.strip_prefix(['-', '+']).unwrap_or(trimmed);
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            "OutOfRange"
        } else {
            "InvalidNumber"
        }
    })
}

/// `float('...')`: only decimal notation is accepted (no `nan` or `inf`).
pub fn parse_float(text: &str) -> Result<f64, &'static str> {
    let trimmed = text.trim();
    if trimmed.trim_start_matches(['-', '+']).starts_with("0x") {
        return match crate::numbers::parse_hex_float(trimmed) {
            Some(n) if n.is_finite() => Ok(n),
            Some(_) => Err("OutOfRange"),
            None => Err("InvalidNumber"),
        };
    }
    let well_formed = !trimmed.is_empty()
        && trimmed.chars().any(|c| c.is_ascii_digit())
        && trimmed.chars().all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'));
    match trimmed.parse::<f64>() {
        Ok(n) if well_formed && n.is_finite() => Ok(n),
        Ok(_) if well_formed => Err("OutOfRange"),
        _ => Err("InvalidNumber"),
    }
}

/// Python-like `istitle`: every word starts with an uppercase letter followed by lowercase ones.
pub fn is_title(s: &str) -> bool {
    let mut cased = false;
    let mut previous_cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            if previous_cased {
                return false;
            }
            previous_cased = true;
            cased = true;
        } else if c.is_lowercase() {
            if !previous_cased {
                return false;
            }
            previous_cased = true;
            cased = true;
        } else {
            previous_cased = false;
        }
    }
    cased
}

/// The int part of a float, or a panic when it does not fit.
pub fn float_to_int(value: f64) -> Result<i64, String> {
    if value.is_finite() && value.abs() < 9.2e18 {
        Ok(value as i64)
    } else {
        Err(format!("integer overflow: {} does not fit in an int", value))
    }
}

// Helpers.

fn text<'a>(string: *const KojiString) -> &'a str {
    string_text(string)
}

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

fn some(option: *const KojiRecordType, value: u64) -> *mut KojiRecord {
    make(option, "Some", &[value])
}

fn none(option: *const KojiRecordType) -> *mut KojiRecord {
    make(option, "None", &[])
}

fn some_string(option: *const KojiRecordType, value: Option<String>) -> *mut KojiRecord {
    match value {
        Some(value) => some(option, slot(new_string(value))),
        None => none(option),
    }
}

/// `Err(<variant>(text))` of a `Result` whose error type has variants with one string.
fn error_with_text(result: *const KojiRecordType, variant: &str, text: &str) -> *mut KojiRecord {
    let error_type = field_record(result, "Err", 0);
    let error = make(error_type, variant, &[slot(new_string(text.to_string()))]);
    make(result, "Err", &[error as usize as u64])
}

// Built-in functions.

/// `input(prompt)`: a line of the standard input, without its line break, or `None` at the
/// end of the input.
#[no_mangle]
pub extern "C" fn koji_std_input(prompt: *const KojiString, option: *const KojiRecordType, location: *const c_char) -> *mut KojiRecord {
    if !prompt.is_null() {
        let mut out = std::io::stdout().lock();
        if let Err(error) = write!(out, "{}", text(prompt)).and_then(|_| out.flush()) {
            drop(out);
            panic_at(location, &format!("cannot write output: {}", error));
        }
    }
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) => none(option),
        Ok(_) => {
            while line.ends_with('\n') || line.ends_with('\r') {
                line.pop();
            }
            some_string(option, Some(line))
        }
        Err(error) => panic_at(location, &format!("cannot read input: {}", error)),
    }
}

/// `panic(message)`.
#[no_mangle]
pub extern "C" fn koji_std_panic(message: *const KojiString, location: *const c_char) -> ! {
    panic_at(location, text(message))
}

#[no_mangle]
pub extern "C" fn koji_std_int_from_string(string: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    match parse_int(text(string)) {
        Ok(n) => make(result, "Ok", &[n as u64]),
        Err(variant) => error_with_text(result, variant, text(string)),
    }
}

#[no_mangle]
pub extern "C" fn koji_std_float_from_string(string: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    match parse_float(text(string)) {
        Ok(n) => make(result, "Ok", &[n.to_bits()]),
        Err(variant) => error_with_text(result, variant, text(string)),
    }
}

#[no_mangle]
pub extern "C" fn koji_std_decimal_from_string(string: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    match crate::decimal::parse(text(string)) {
        Ok(n) => {
            let (lo, hi) = n.to_bits();
            make(result, "Ok", &[lo, hi])
        }
        Err(crate::decimal::ParseFailure::Invalid) => error_with_text(result, "InvalidNumber", text(string)),
        Err(crate::decimal::ParseFailure::OutOfRange) => error_with_text(result, "OutOfRange", text(string)),
    }
}

#[no_mangle]
pub extern "C" fn koji_std_float_to_int(value: f64, location: *const c_char) -> i64 {
    float_to_int(value.trunc()).unwrap_or_else(|message| panic_at(location, &message))
}

// Methods of strings.

#[no_mangle]
pub extern "C" fn koji_std_string_upper(string: *const KojiString) -> *mut KojiString {
    new_string(text(string).to_uppercase())
}

#[no_mangle]
pub extern "C" fn koji_std_string_lower(string: *const KojiString) -> *mut KojiString {
    new_string(text(string).to_lowercase())
}

/// The `is...` methods: `test` is 0 `isSpace`, 1 `isNumeric`, 2 `isAlpha`, 3
/// `isAlphaNumeric`, 4 `isUpper`, 5 `isLower`, 6 `isTitle`.
#[no_mangle]
pub extern "C" fn koji_std_string_is(string: *const KojiString, test: i32) -> bool {
    let s = text(string);
    let all = |predicate: fn(char) -> bool| !s.is_empty() && s.chars().all(predicate);
    let has_cased = s.chars().any(|c| c.is_uppercase() || c.is_lowercase());
    match test {
        0 => all(char::is_whitespace),
        1 => all(char::is_numeric),
        2 => all(char::is_alphabetic),
        3 => all(char::is_alphanumeric),
        4 => has_cased && !s.chars().any(char::is_lowercase),
        5 => has_cased && !s.chars().any(char::is_uppercase),
        _ => is_title(s),
    }
}

// char

/// `char(code)`: `Some` of the character with that code, or `None`.
#[no_mangle]
pub extern "C" fn koji_std_char(code: i64, option: *const KojiRecordType) -> *mut KojiRecord {
    match u32::try_from(code).ok().and_then(char::from_u32) {
        Some(c) => some(option, c as u32 as u64),
        None => none(option),
    }
}

/// `text.chars()`.
#[no_mangle]
pub extern "C" fn koji_std_string_chars(string: *const KojiString, element: *const KojiElementType) -> *mut KojiList {
    new_list(element, text(string).chars().map(|c| c as u32 as u64).collect())
}

/// `text[index]`: the character at that position (in characters).
#[no_mangle]
pub extern "C" fn koji_std_string_char_at(string: *const KojiString, index: i64, location: *const c_char) -> u32 {
    let s = text(string);
    let length = s.chars().count();
    if index < 0 || index as usize >= length {
        panic_at(location, &format!("index {} out of range (length {})", index, length));
    }
    s.chars().nth(index as usize).expect("checked") as u32
}

/// `text[start..end]` (in characters); a missing bound is the start or the end.
#[no_mangle]
pub extern "C" fn koji_std_string_slice(
    string: *const KojiString,
    has_start: bool,
    start: i64,
    has_end: bool,
    end: i64,
    location: *const c_char,
) -> *mut KojiString {
    let s = text(string);
    let length = s.chars().count() as i64;
    let (start, end) = (if has_start { start } else { 0 }, if has_end { end } else { length });
    if start < 0 || end > length || start > end {
        panic_at(location, &format!("slice {}..{} out of range (length {})", start, end, length));
    }
    new_string(s.chars().skip(start as usize).take((end - start) as usize).collect())
}
