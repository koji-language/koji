//! What `std/system.kj` needs from the system that its C library does not give in the same way
//! everywhere: the arguments of `main`, `errno`, the environment and the path of the executable.

use std::ffi::{c_char, CString};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::OnceLock;

static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const c_char> = AtomicPtr::new(std::ptr::null_mut());

/// Keeps the arguments of `main` (see `koji_init`).
pub(crate) fn keep_arguments(argc: i32, argv: *const *const c_char) {
    ARGC.store(argc.max(0) as usize, Ordering::Relaxed);
    ARGV.store(argv as *mut *const c_char, Ordering::Relaxed);
}

/// The arguments after the program itself.
#[no_mangle]
pub extern "C" fn koji_system_argument_count() -> i64 {
    ARGC.load(Ordering::Relaxed).saturating_sub(1) as i64
}

/// The argument `index` after the program itself, as a string of C.
#[no_mangle]
pub extern "C" fn koji_system_argument(index: i64) -> *const c_char {
    unsafe { *ARGV.load(Ordering::Relaxed).add(index as usize + 1) }
}

/// The error of the last call of the C library that failed.
#[no_mangle]
pub extern "C" fn koji_system_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

extern "C" {
    static environ: *const *const c_char;
}

/// The environment, for the programs this one runs.
#[no_mangle]
pub extern "C" fn koji_system_environment() -> *const *const c_char {
    unsafe { environ }
}

/// The path of the executable of this program, or NULL where the system cannot tell it.
#[no_mangle]
pub extern "C" fn koji_system_executable() -> *const c_char {
    static PATH: OnceLock<Option<CString>> = OnceLock::new();
    let path = PATH.get_or_init(|| {
        let path = std::env::current_exe().ok()?;
        CString::new(path.to_string_lossy().into_owned()).ok()
    });
    path.as_ref().map_or(std::ptr::null(), |path| path.as_ptr())
}
