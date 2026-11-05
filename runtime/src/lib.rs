//! The runtime of compiled Koji programs.
//!
//! `kojic build` translates a program to C, and the C code calls the functions of this crate,
//! which is linked into the program as a static library. They follow the C ABI and are declared
//! in `prelude.h`, which starts every C file the compiler generates (the compiler written in Koji
//! has its lines in `compiler/prelude.kj`).

mod closure;
mod decimal;
pub mod datetime;
mod file;
mod hash;
mod http;
mod json;
mod list;
mod map;
mod record;
mod regex;
mod stdlib;
mod numbers;
mod output;
mod pool;
mod string;

pub use numbers::{float_pow, format_float, int_pow, parse_hex_float, PowFailure};
pub use string::repr_string;
pub use http::{http_request, HttpOutcome};
pub use json::{parse_json, write_decimal, write_string, JsonFailure, JsonTree};
pub use regex::regex_error_message;
pub use stdlib::{error_message, float_to_int, is_title, operating_system, parse_float, parse_int};
