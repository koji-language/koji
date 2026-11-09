//! The runtime of compiled Koji programs.
//!
//! `kojic build` translates a program to C, and the C code calls the functions of this crate,
//! which is linked into the program as a static library. They follow the C ABI and are declared
//! in `prelude.h`, which starts every C file the compiler generates (the compiler written in Koji
//! has its lines in `compiler/prelude.kj`).

mod closure;
mod hash;
mod http;
mod list;
mod map;
mod record;
mod stdlib;
mod output;
mod pool;
mod string;
mod system;

pub use string::repr_string;
pub use http::{http_request, HttpOutcome};
pub use stdlib::error_message;
