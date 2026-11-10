//! The layout of strings, for the runtime in Rust that still reads and makes them (`http.rs`):
//! strings are in std/koji/text.kj and the prelude.

/// A string: its count of references, its size, the bytes its block holds, then its UTF-8 bytes.
#[repr(C)]
pub struct KojiString {
    references: usize,
    length: usize,
    capacity: usize,
    bytes: [u8; 0],
}

extern "C" {
    fn koji_string_from_utf8(bytes: *const u8, length: usize) -> *mut KojiString;
}

pub(crate) fn new_string(text: String) -> *mut KojiString {
    unsafe { koji_string_from_utf8(text.as_ptr(), text.len()) }
}

pub(crate) fn string_text<'a>(string: *const KojiString) -> &'a str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts((*string).bytes.as_ptr(), (*string).length)) }
}
