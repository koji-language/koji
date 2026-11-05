//! Strings: immutable text with a reference count, shared between copies (value semantics with
//! copy-on-write: a copy only increments the count).
//!
//! The generated code owns one reference per variable that holds a string. `retain` adds a
//! reference and `release` removes one, freeing the string at zero. Functions that return a
//! string return a new reference.

use std::cell::Cell;
use std::cmp::Ordering;

use crate::numbers::format_float;

/// A string, laid out for C (the prelude counts its references inline): its UTF-8 bytes follow
/// it, in the same allocation, from the pool for a short one.
#[repr(C)]
pub struct KojiString {
    references: Cell<usize>,
    length: usize,
    bytes: [u8; 0],
}

impl KojiString {
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes.as_ptr(), self.length)) }
    }
}

/// The 8-byte words of a string of `length` bytes.
fn block_words(length: usize) -> usize {
    std::mem::size_of::<KojiString>() / 8 + length.div_ceil(8)
}

/// A new string of `length` bytes that `fill` writes (they must be UTF-8).
fn allocate(length: usize, fill: impl FnOnce(&mut [u8])) -> *mut KojiString {
    unsafe {
        let string = crate::pool::take(block_words(length)) as *mut KojiString;
        string.write(KojiString { references: Cell::new(1), length, bytes: [] });
        fill(std::slice::from_raw_parts_mut((*string).bytes.as_mut_ptr(), length));
        string
    }
}

pub(crate) fn new_string(text: String) -> *mut KojiString {
    from_str(&text)
}

fn from_str(text: &str) -> *mut KojiString {
    allocate(text.len(), |bytes| bytes.copy_from_slice(text.as_bytes()))
}

pub(crate) fn string_text<'a>(string: *const KojiString) -> &'a str {
    unsafe { (*string).as_str() }
}

/// A new string with the UTF-8 bytes `bytes[..length]`.
#[no_mangle]
pub extern "C" fn koji_string_new(bytes: *const u8, length: usize) -> *mut KojiString {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
    from_str(&String::from_utf8_lossy(bytes))
}

/// Frees a string without references (the prelude releases strings inline).
#[no_mangle]
pub extern "C" fn koji_string_free(string: *mut KojiString) {
    let length = unsafe { (*string).length };
    crate::pool::give(string as *mut u8, block_words(length));
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
            koji_string_free(string);
        } else {
            reference.references.set(references);
        }
    }
}

#[no_mangle]
pub extern "C" fn koji_string_concat(a: *const KojiString, b: *const KojiString) -> *mut KojiString {
    let (a, b) = (string_text(a), string_text(b));
    allocate(a.len() + b.len(), |bytes| {
        bytes[..a.len()].copy_from_slice(a.as_bytes());
        bytes[a.len()..].copy_from_slice(b.as_bytes());
    })
}

/// `text * count` (an empty string for a count below 1).
#[no_mangle]
pub extern "C" fn koji_string_repeat(string: *const KojiString, count: i64) -> *mut KojiString {
    new_string(string_text(string).repeat(count.max(0) as usize))
}

#[no_mangle]
pub extern "C" fn koji_string_equal(a: *const KojiString, b: *const KojiString) -> bool {
    string_text(a) == string_text(b)
}

/// -1, 0 or 1, comparing the bytes.
#[no_mangle]
pub extern "C" fn koji_string_compare(a: *const KojiString, b: *const KojiString) -> i32 {
    match string_text(a).cmp(string_text(b)) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[no_mangle]
pub extern "C" fn koji_string_from_int(value: i64) -> *mut KojiString {
    new_string(value.to_string())
}

#[no_mangle]
pub extern "C" fn koji_string_from_float(value: f64) -> *mut KojiString {
    new_string(format_float(value))
}

#[no_mangle]
pub extern "C" fn koji_string_from_bool(value: bool) -> *mut KojiString {
    new_string(value.to_string())
}

/// The text as written in code: in quotes, with `\\`, `\'`, `\n`, `\t` and `\r` escaped.
pub fn repr_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

#[no_mangle]
pub extern "C" fn koji_string_repr(string: *const KojiString) -> *mut KojiString {
    new_string(repr_string(string_text(string)))
}

/// The number of characters (not bytes).
#[no_mangle]
pub extern "C" fn koji_string_length(string: *const KojiString) -> i64 {
    string_text(string).chars().count() as i64
}

/// `needle in haystack`.
#[no_mangle]
pub extern "C" fn koji_string_contains(haystack: *const KojiString, needle: *const KojiString) -> bool {
    string_text(haystack).contains(string_text(needle))
}

#[no_mangle]
pub extern "C" fn koji_string_hash(string: *const KojiString) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    string_text(string).hash(&mut hasher);
    hasher.finish()
}

fn char_of(code: u32) -> char {
    char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// The text of a char (`print`).
#[no_mangle]
pub extern "C" fn koji_string_from_char(code: u32) -> *mut KojiString {
    let c = char_of(code);
    allocate(c.len_utf8(), |bytes| {
        c.encode_utf8(bytes);
    })
}

/// A char as written in code: `'a'`, `'\n'`.
#[no_mangle]
pub extern "C" fn koji_char_repr(code: u32) -> *mut KojiString {
    let mut text = String::from("'");
    match char_of(code) {
        '\'' => text.push_str("\\'"),
        '\\' => text.push_str("\\\\"),
        '\n' => text.push_str("\\n"),
        '\t' => text.push_str("\\t"),
        '\r' => text.push_str("\\r"),
        other => text.push(other),
    }
    text.push('\'');
    new_string(text)
}

/// `c in text`.
#[no_mangle]
pub extern "C" fn koji_string_contains_char(haystack: *const KojiString, code: u32) -> bool {
    string_text(haystack).contains(char_of(code))
}
