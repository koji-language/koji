//! The runtime of compiled Koji programs.
//!
//! `kojic build` translates a program to C, and the C code calls the functions of this crate,
//! which is linked into the program as a static library. They follow the C ABI and are declared
//! in the header the C generator writes (`src/backend/c/prelude.h` in `kojic`).
//!
//! The interpreter uses the same functions (`format_float`, `repr_string`, `int_pow`, `float_pow`), so both
//! produce exactly the same output.

mod closure;
mod list;
mod map;
mod record;
mod numbers;
mod output;
mod string;

pub use numbers::{float_pow, format_float, int_pow, PowFailure};
pub use string::repr_string;
