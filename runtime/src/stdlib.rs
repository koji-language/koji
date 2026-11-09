//! The start of a program, and the messages of the errors of the standard library (for the
//! display of records, until they move).

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

// The messages of errors and the conversions of text.

/// The readable message of an error of the standard library (`'x' is not a valid number`),
/// from the texts of its fields; `None` for any other type.
pub fn error_message(type_name: &str, variant: &str, texts: &[String]) -> Option<String> {
    const ERROR_TYPES: &[&str] = &[
        "ParseError",
        "HttpError",
    ];
    if !ERROR_TYPES.contains(&type_name) {
        return None;
    }
    let text = |i: usize| texts.get(i).cloned().unwrap_or_default();
    Some(match variant {
        "InvalidNumber" => format!("'{}' is not a valid number", text(0)),
        "OutOfRange" => format!("'{}' is out of range", text(0)),
        "InvalidUrl" => format!("invalid URL '{}'", text(0)),
        "ConnectionFailed" => format!("cannot connect to '{}': {}", text(0), text(1)),
        "Timeout" => format!("the request to '{}' timed out", text(0)),
        "Tls" => format!("secure connection to '{}' failed: {}", text(0), text(1)),
        "InvalidResponse" => format!("invalid response from '{}': {}", text(0), text(1)),
        _ => return None,
    })
}
