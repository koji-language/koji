//! The standard library for compiled programs: the built-in functions (`input`, `int`,
//! `float`) and the methods of strings that are not written in Koji.
//!
//! A function that gives a value of a declared type (`Option`, `Result`, an error) receives
//! its description from the generated code and builds it with `record::make`. The messages
//! of errors and the conversions of text come first.

use std::ffi::c_char;
use std::io::{BufRead, Write};

use crate::output::panic_at;
use crate::record::{make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

/// Called by `main` before anything else.
#[no_mangle]
pub extern "C" fn koji_init(argc: i32, argv: *const *const c_char) {
    crate::pool::adopt();
    crate::system::keep_arguments(argc, argv);
    unsafe { koji_library_init() };
}

extern "C" {
    /// The library of the runtime written in Koji (`kojic dump library`), which build.rs adds to
    /// this one: its modules run first.
    fn koji_library_init();
}

/// The tests of this crate run without the library written in Koji.
#[cfg(test)]
mod without_library {
    #[no_mangle]
    extern "C" fn koji_library_init() {}
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

