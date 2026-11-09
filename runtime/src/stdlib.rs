//! The standard library for compiled programs: the built-in functions (`input`, `int`,
//! `float`), the methods of strings and the module `system`.
//!
//! A function that gives a value of a declared type (`Option`, `Result`, an error) receives
//! its description from the generated code and builds it with `record::make`. The messages
//! of errors and the conversions of text come first.

use std::ffi::{c_char, CStr};
use std::io::{BufRead, Write};
use std::sync::OnceLock;

use crate::list::{new_list, KojiElementType, KojiList};
use crate::output::panic_at;
use crate::record::{field_record, make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

/// The program's command-line arguments, after the program itself.
static ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Called by `main` before anything else.
#[no_mangle]
pub extern "C" fn koji_init(argc: i32, argv: *const *const c_char) {
    crate::pool::adopt();
    let args = (1..argc.max(1) as usize)
        .map(|index| unsafe { CStr::from_ptr(*argv.add(index)) }.to_string_lossy().into_owned())
        .collect();
    let _ = ARGS.set(args);
}

// The messages of errors and the conversions of text.

/// The readable message of an error of the standard library (`'x' is not a valid number`),
/// from the texts of its fields; `None` for any other type.
pub fn error_message(type_name: &str, variant: &str, texts: &[String]) -> Option<String> {
    const ERROR_TYPES: &[&str] = &[
        "ParseError",
        "MathError",
        "FileError",
        "HttpError",
        "SystemError",
    ];
    if !ERROR_TYPES.contains(&type_name) {
        return None;
    }
    let text = |i: usize| texts.get(i).cloned().unwrap_or_default();
    if type_name == "SystemError" {
        return Some(match variant {
            "NotFound" => format!("command '{}' not found", text(0)),
            "PermissionDenied" => format!("permission denied to run '{}'", text(0)),
            _ => format!("cannot run '{}': {}", text(0), text(1)),
        });
    }
    Some(match variant {
        "InvalidNumber" => format!("'{}' is not a valid number", text(0)),
        "OutOfRange" => format!("'{}' is out of range", text(0)),
        "Complex" => "the result is not a real number (it is complex)".to_string(),
        "Indeterminate" => "the result is indeterminate".to_string(),
        "Infinite" => "the result is infinite".to_string(),
        "NotFound" => format!("file '{}' not found", text(0)),
        "PermissionDenied" => format!("permission denied for '{}'", text(0)),
        "IsDirectory" => format!("'{}' is a directory", text(0)),
        "InvalidUtf8" => format!("file '{}' is not valid UTF-8 text", text(0)),
        "Io" => format!("cannot access '{}': {}", text(0), text(1)),
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

fn strings(element: *const KojiElementType, items: impl Iterator<Item = String>) -> *mut KojiList {
    new_list(element, items.map(|item| slot(new_string(item))).collect())
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
pub extern "C" fn koji_std_string_trim(string: *const KojiString) -> *mut KojiString {
    new_string(text(string).trim().to_string())
}

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

#[no_mangle]
pub extern "C" fn koji_std_string_split(
    string: *const KojiString,
    separator: *const KojiString,
    element: *const KojiElementType,
    location: *const c_char,
) -> *mut KojiList {
    let separator = text(separator);
    if separator.is_empty() {
        panic_at(location, "split() separator cannot be empty");
    }
    strings(element, text(string).split(separator).map(str::to_string))
}

#[no_mangle]
pub extern "C" fn koji_std_string_replace(
    string: *const KojiString,
    old: *const KojiString,
    new: *const KojiString,
    location: *const c_char,
) -> *mut KojiString {
    if text(old).is_empty() {
        panic_at(location, "replace() needs a non-empty text to replace");
    }
    new_string(text(string).replace(text(old), text(new)))
}

#[no_mangle]
pub extern "C" fn koji_std_string_starts_with(string: *const KojiString, prefix: *const KojiString) -> bool {
    text(string).starts_with(text(prefix))
}

#[no_mangle]
pub extern "C" fn koji_std_string_ends_with(string: *const KojiString, suffix: *const KojiString) -> bool {
    text(string).ends_with(text(suffix))
}

/// A position in characters, like indexing and `length()`.
#[no_mangle]
pub extern "C" fn koji_std_string_index_of(string: *const KojiString, part: *const KojiString, option: *const KojiRecordType) -> *mut KojiRecord {
    let s = text(string);
    match s.find(text(part)) {
        Some(byte) => some(option, s[..byte].chars().count() as u64),
        None => none(option),
    }
}

// system

#[no_mangle]
pub extern "C" fn koji_std_system_args(element: *const KojiElementType) -> *mut KojiList {
    strings(element, ARGS.get().cloned().unwrap_or_default().into_iter())
}

#[no_mangle]
pub extern "C" fn koji_std_system_env(name: *const KojiString, option: *const KojiRecordType) -> *mut KojiRecord {
    some_string(option, std::env::var(text(name)).ok())
}

/// `system.executable()`: the path of the executable of this program, or `None` where the system
/// cannot tell it.
#[no_mangle]
pub extern "C" fn koji_std_system_executable(option: *const KojiRecordType) -> *mut KojiRecord {
    let path = std::env::current_exe().ok().map(|path| path.to_string_lossy().into_owned());
    some_string(option, path)
}

/// The variant of `OperatingSystem` for the system the program runs on (`Linux`, `MacOS` or
/// `Windows`), or, for another one, its name for `Other(name)`.
pub fn operating_system() -> Result<&'static str, &'static str> {
    match std::env::consts::OS {
        "linux" => Ok("Linux"),
        "macos" => Ok("MacOS"),
        "windows" => Ok("Windows"),
        other => Err(other),
    }
}

/// `system.os`, a value of the `OperatingSystem` record `os`.
#[no_mangle]
pub extern "C" fn koji_std_system_os(os: *const KojiRecordType) -> *mut KojiRecord {
    match operating_system() {
        Ok(variant) => make(os, variant, &[]),
        Err(name) => make(os, "Other", &[slot(new_string(name.to_string()))]),
    }
}

#[no_mangle]
pub extern "C" fn koji_std_system_exit(code: i64) -> ! {
    let _ = std::io::stdout().flush();
    std::process::exit(code.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

/// The texts of a list of strings.
fn texts(list: *const KojiList) -> Vec<String> {
    let list = unsafe { &*list };
    list.words().iter().map(|item| text(*item as usize as *const KojiString).to_string()).collect()
}

/// `Err(SystemError)` for a program that could not be run.
fn system_error(result: *const KojiRecordType, command: &str, error: &std::io::Error) -> *mut KojiRecord {
    use std::io::ErrorKind;
    let error_type = field_record(result, "Err", 0);
    let name = slot(new_string(command.to_string()));
    let error = match error.kind() {
        ErrorKind::NotFound => make(error_type, "NotFound", &[name]),
        ErrorKind::PermissionDenied => make(error_type, "PermissionDenied", &[name]),
        _ => make(error_type, "Io", &[name, slot(new_string(error.to_string()))]),
    };
    make(result, "Err", &[error as usize as u64])
}

/// `system.run(command, arguments)`, with the terminal of this program: `Ok` of its exit code
/// (`-1` if a signal stopped it) or `Err(SystemError)`.
#[no_mangle]
pub extern "C" fn koji_std_system_run(command: *const KojiString, arguments: *const KojiList, result: *const KojiRecordType) -> *mut KojiRecord {
    let command = text(command).to_string();
    let _ = std::io::stdout().flush();
    match std::process::Command::new(&command).args(texts(arguments)).status() {
        Ok(status) => make(result, "Ok", &[status.code().unwrap_or(-1) as i64 as u64]),
        Err(error) => system_error(result, &command, &error),
    }
}

/// `system.capture(command, arguments)`: `Ok(Output)`, with what it printed, or `Err(SystemError)`.
#[no_mangle]
pub extern "C" fn koji_std_system_capture(command: *const KojiString, arguments: *const KojiList, result: *const KojiRecordType) -> *mut KojiRecord {
    let command = text(command).to_string();
    let arguments = texts(arguments);
    let _ = std::io::stdout().flush();
    match std::process::Command::new(&command).args(&arguments).output() {
        Ok(output) => {
            let output_type = field_record(result, "Ok", 0);
            let value = make(
                output_type,
                "",
                &[
                    output.status.code().unwrap_or(-1) as i64 as u64,
                    slot(new_string(String::from_utf8_lossy(&output.stdout).into_owned())),
                    slot(new_string(String::from_utf8_lossy(&output.stderr).into_owned())),
                ],
            );
            make(result, "Ok", &[value as usize as u64])
        }
        Err(error) => system_error(result, &command, &error),
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
