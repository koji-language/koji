//! The panics of the runtime in Rust (the output and the panics of programs are in
//! std/koji/output.kj).

use std::ffi::{c_char, CStr};
use std::io::Write;

/// Ends the program with `panic: <location>: <message>` on the standard error, and exit code 1.
pub(crate) fn panic_at(location: *const c_char, message: &str) -> ! {
    let location = unsafe { CStr::from_ptr(location) }.to_string_lossy();
    let _ = std::io::stdout().flush();
    eprintln!("panic: {}: {}", location, message);
    std::process::exit(1)
}
