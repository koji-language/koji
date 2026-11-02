//! Strings: immutable text with a reference count, shared between copies (value semantics with
//! copy-on-write: a copy only increments the count).
//!
//! The generated code owns one reference per variable that holds a string. `retain` adds a
//! reference and `release` removes one, freeing the string at zero. Functions that return a
//! string return a new reference.

use std::cell::Cell;
use std::cmp::Ordering;

use crate::numbers::format_float;

pub struct KojiString {
    references: Cell<usize>,
    text: String,
}

impl KojiString {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

fn new(text: String) -> *mut KojiString {
    Box::into_raw(Box::new(KojiString {
        references: Cell::new(1),
        text,
    }))
}

fn text<'a>(string: *const KojiString) -> &'a str {
    unsafe { &(*string).text }
}

/// A new string with the UTF-8 bytes `bytes[..length]`.
#[no_mangle]
pub extern "C" fn koji_string_new(bytes: *const u8, length: usize) -> *mut KojiString {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
    new(String::from_utf8_lossy(bytes).into_owned())
}

#[no_mangle]
pub extern "C" fn koji_string_retain(string: *const KojiString) {
    if let Some(string) = unsafe { string.as_ref() } {
        string.references.set(string.references.get() + 1);
    }
}

/// Removes a reference; `NULL` is ignored (a variable that was never assigned).
#[no_mangle]
pub extern "C" fn koji_string_release(string: *mut KojiString) {
    if let Some(reference) = unsafe { string.as_ref() } {
        let references = reference.references.get() - 1;
        if references == 0 {
            drop(unsafe { Box::from_raw(string) });
        } else {
            reference.references.set(references);
        }
    }
}

#[no_mangle]
pub extern "C" fn koji_string_concat(a: *const KojiString, b: *const KojiString) -> *mut KojiString {
    let (a, b) = (text(a), text(b));
    let mut result = String::with_capacity(a.len() + b.len());
    result.push_str(a);
    result.push_str(b);
    new(result)
}

/// `text * count` (an empty string for a count below 1).
#[no_mangle]
pub extern "C" fn koji_string_repeat(string: *const KojiString, count: i64) -> *mut KojiString {
    new(text(string).repeat(count.max(0) as usize))
}

#[no_mangle]
pub extern "C" fn koji_string_equal(a: *const KojiString, b: *const KojiString) -> bool {
    text(a) == text(b)
}

/// -1, 0 or 1, comparing the bytes.
#[no_mangle]
pub extern "C" fn koji_string_compare(a: *const KojiString, b: *const KojiString) -> i32 {
    match text(a).cmp(text(b)) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[no_mangle]
pub extern "C" fn koji_string_from_int(value: i64) -> *mut KojiString {
    new(value.to_string())
}

#[no_mangle]
pub extern "C" fn koji_string_from_float(value: f64) -> *mut KojiString {
    new(format_float(value))
}

#[no_mangle]
pub extern "C" fn koji_string_from_bool(value: bool) -> *mut KojiString {
    new(value.to_string())
}
