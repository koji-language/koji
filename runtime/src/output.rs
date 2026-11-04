//! Output and panics.

use std::ffi::{c_char, CStr};
use std::io::Write;

use crate::string::KojiString;

/// Ends the program with `panic: <location>: <message>` on the standard error, like the
/// interpreter, and exit code 1.
pub(crate) fn panic_at(location: *const c_char, message: &str) -> ! {
    let location = unsafe { CStr::from_ptr(location) }.to_string_lossy();
    let _ = std::io::stdout().flush();
    eprintln!("panic: {}: {}", location, message);
    std::process::exit(1)
}

/// `panic` raised by the generated code (overflow, division by zero, ...).
#[no_mangle]
pub extern "C" fn koji_panic(location: *const c_char, message: *const c_char) -> ! {
    let message = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    panic_at(location, &message)
}

/// `a << b` or `a >> b` with a shift amount outside 0..64.
#[no_mangle]
pub extern "C" fn koji_panic_shift(location: *const c_char, amount: i64) -> ! {
    panic_at(location, &format!("invalid shift amount {}", amount))
}

/// Writes `text` and a line break to the standard output (`print`).
#[no_mangle]
pub extern "C" fn koji_print(text: *const KojiString, location: *const c_char) {
    let text = unsafe { &*text };
    let mut out = std::io::stdout().lock();
    if let Err(error) = writeln!(out, "{}", text.as_str()).and_then(|_| out.flush()) {
        drop(out);
        panic_at(location, &format!("cannot write output: {}", error));
    }
}

/// Writes `text` and a line break to the standard error output (`printError`), after what
/// `print` wrote.
#[no_mangle]
pub extern "C" fn koji_print_error(text: *const KojiString, location: *const c_char) {
    let text = unsafe { &*text };
    let _ = std::io::stdout().flush();
    let mut errors = std::io::stderr().lock();
    if let Err(error) = writeln!(errors, "{}", text.as_str()).and_then(|_| errors.flush()) {
        drop(errors);
        panic_at(location, &format!("cannot write output: {}", error));
    }
}

/// An `Err` or `None` that reached the top level of the program with `?`.
#[no_mangle]
pub extern "C" fn koji_fail(location: *const c_char, message: *const KojiString) -> ! {
    let location = unsafe { CStr::from_ptr(location) }.to_string_lossy();
    let message = unsafe { &*message }.as_str();
    let _ = std::io::stdout().flush();
    eprintln!("error: {}: {}", location, message);
    std::process::exit(1)
}
