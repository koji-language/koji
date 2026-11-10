//! The layout of strings, for `http.rs`, which reads them: strings are in std/koji/text.kj and the
//! prelude.

/// A string: its count of references, its size, the bytes its block holds, then its UTF-8 bytes.
#[repr(C)]
pub struct KojiString {
    references: usize,
    length: usize,
    capacity: usize,
    bytes: [u8; 0],
}

pub(crate) fn string_text<'a>(string: *const KojiString) -> &'a str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts((*string).bytes.as_ptr(), (*string).length)) }
}
