//! What is left in Rust of the runtime of compiled Koji programs: HTTPS (`https://` of std/http.kj,
//! with the `ureq` library), until TLS is written in Koji. The rest of the runtime is the library
//! written in Koji (std/koji/), whose C build.rs adds to this archive, and the prelude
//! (`prelude.h`), which starts every C file the compiler generates.

mod http;
mod string;

pub use http::{http_request, HttpOutcome};
