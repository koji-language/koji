//! The start of a program.

use std::ffi::c_char;

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
