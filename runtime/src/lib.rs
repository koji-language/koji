//! What is left in Rust of the runtime of compiled Koji programs: HTTP (`http.get` and the others,
//! with the `ureq` library), until TLS is written in Koji. The rest of the runtime is the library
//! written in Koji (std/koji/), whose C build.rs adds to this archive, and the prelude
//! (`prelude.h`), which starts every C file the compiler generates.

mod http;
mod record;
mod string;

pub use http::{http_request, HttpOutcome};

/// The tests of this crate run without the library written in Koji.
#[cfg(test)]
mod without_library {
    #[no_mangle]
    extern "C" fn koji_pool_take_block(_words: usize) -> *mut u8 {
        std::ptr::null_mut()
    }
    #[no_mangle]
    extern "C" fn koji_string_from_utf8(_bytes: *const u8, _length: usize) -> *mut crate::string::KojiString {
        std::ptr::null_mut()
    }
}
