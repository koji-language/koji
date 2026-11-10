//! The start of a program.

use std::ffi::c_char;

/// Called by `main` before anything else.
#[no_mangle]
pub extern "C" fn koji_init(argc: i32, argv: *const *const c_char) {
    unsafe { koji_pool_start() };
    crate::system::keep_arguments(argc, argv);
    unsafe { koji_library_init() };
}

extern "C" {
    /// The library of the runtime written in Koji (`kojic dump library`), which build.rs adds to
    /// this one: its modules run first, after the pool reads `KOJI_POOL` (std/koji/memory.kj).
    fn koji_library_init();
    fn koji_pool_start();
}

/// The tests of this crate run without the library written in Koji.
#[cfg(test)]
mod without_library {
    #[no_mangle]
    extern "C" fn koji_library_init() {}
    #[no_mangle]
    extern "C" fn koji_pool_start() {}
    #[no_mangle]
    extern "C" fn koji_pool_take_block(_words: usize) -> *mut u8 {
        std::ptr::null_mut()
    }
    #[no_mangle]
    extern "C" fn koji_string_from_utf8(_bytes: *const u8, _length: usize) -> *mut crate::string::KojiString {
        std::ptr::null_mut()
    }
}
