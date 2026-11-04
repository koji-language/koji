//! The `regex` module: compiled regular expressions (`Regex`) and their methods.

use std::cell::Cell;

use crate::list::{new_list, KojiElementType, KojiList};
use crate::record::{field_record, make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

pub struct KojiRegex {
    references: Cell<usize>,
    regex: regex::Regex,
}

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

/// The last line of the `regex` crate's message, without its `error: ` prefix.
pub fn regex_error_message(error: &regex::Error) -> String {
    let text = error.to_string();
    let last = text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("");
    last.trim().trim_start_matches("error: ").to_string()
}

/// `regex.compile(pattern)`: `Ok(Regex)` or `Err(RegexError)`.
#[no_mangle]
pub extern "C" fn koji_std_regex_compile(pattern: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    let pattern = string_text(pattern);
    match regex::Regex::new(pattern) {
        Ok(regex) => {
            let value = Box::into_raw(Box::new(KojiRegex { references: Cell::new(1), regex }));
            make(result, "Ok", &[value as usize as u64])
        }
        Err(error) => {
            let error_type = field_record(result, "Err", 0);
            let pattern_slot = slot(new_string(pattern.to_string()));
            let value = match &error {
                regex::Error::CompiledTooBig(_) => make(error_type, "TooLarge", &[pattern_slot]),
                _ => {
                    let message = regex_error_message(&error);
                    let name = if message.contains("not supported") { "Unsupported" } else { "InvalidPattern" };
                    make(error_type, name, &[pattern_slot, slot(new_string(message))])
                }
            };
            make(result, "Err", &[value as usize as u64])
        }
    }
}

#[no_mangle]
pub extern "C" fn koji_regex_retain(regex: *const KojiRegex) {
    if let Some(regex) = unsafe { regex.as_ref() } {
        regex.references.set(regex.references.get() + 1);
    }
}

#[no_mangle]
pub extern "C" fn koji_regex_release(regex: *mut KojiRegex) {
    let Some(reference) = (unsafe { regex.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    drop(unsafe { Box::from_raw(regex) });
}

fn regex<'a>(regex: *const KojiRegex) -> &'a regex::Regex {
    unsafe { &(*regex).regex }
}

/// `<regex '[0-9]+'>`.
#[no_mangle]
pub extern "C" fn koji_regex_repr(value: *const KojiRegex) -> *mut KojiString {
    new_string(format!("<regex '{}'>", regex(value).as_str()))
}

#[no_mangle]
pub extern "C" fn koji_std_regex_test(value: *const KojiRegex, input: *const KojiString) -> bool {
    regex(value).is_match(string_text(input))
}

/// A `Match`: its text and its positions in characters.
fn match_record(ty: *const KojiRecordType, input: &str, found: &regex::Match) -> *mut KojiRecord {
    let start = input[..found.start()].chars().count() as i64;
    let end = start + found.as_str().chars().count() as i64;
    make(ty, "", &[slot(new_string(found.as_str().to_string())), start as u64, end as u64])
}

/// `regex.find(input)`: `Some(Match)` or `None`.
#[no_mangle]
pub extern "C" fn koji_std_regex_find(value: *const KojiRegex, input: *const KojiString, option: *const KojiRecordType) -> *mut KojiRecord {
    let input = string_text(input);
    match regex(value).find(input) {
        Some(found) => {
            let found = match_record(field_record(option, "Some", 0), input, &found);
            make(option, "Some", &[found as usize as u64])
        }
        None => make(option, "None", &[]),
    }
}

/// `regex.findAll(input)`: the matches, as a list of `Match` (described by `ty`).
#[no_mangle]
pub extern "C" fn koji_std_regex_find_all(
    value: *const KojiRegex,
    input: *const KojiString,
    element: *const KojiElementType,
    ty: *const KojiRecordType,
) -> *mut KojiList {
    let input = string_text(input);
    let items = regex(value)
        .find_iter(input)
        .map(|found| match_record(ty, input, &found) as usize as u64)
        .collect();
    new_list(element, items)
}

/// `regex.replace` (the first match, `all` false) and `regex.replaceAll`.
#[no_mangle]
pub extern "C" fn koji_std_regex_replace(
    value: *const KojiRegex,
    input: *const KojiString,
    replacement: *const KojiString,
    all: bool,
) -> *mut KojiString {
    let (input, replacement) = (string_text(input), string_text(replacement));
    let result = if all {
        regex(value).replace_all(input, replacement)
    } else {
        regex(value).replace(input, replacement)
    };
    new_string(result.into_owned())
}

#[no_mangle]
pub extern "C" fn koji_std_regex_split(value: *const KojiRegex, input: *const KojiString, element: *const KojiElementType) -> *mut KojiList {
    let items = regex(value)
        .split(string_text(input))
        .map(|part| slot(new_string(part.to_string())))
        .collect();
    new_list(element, items)
}
