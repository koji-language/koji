//! HTTPS for std/http.kj, with the `ureq` library, until TLS is written in Koji.

use std::alloc::GlobalAlloc;

use crate::string::{string_text, KojiString};

/// The answer to a request: the status and the body, or the variant of `HttpError` with the
/// URL and, for some variants, a message.
#[derive(Debug, Clone, PartialEq)]
pub enum HttpOutcome {
    Response { status: i64, body: String },
    Error { variant: &'static str, url: String, message: Option<String> },
}

fn transport_error(url: &str, error: &ureq::Transport) -> HttpOutcome {
    use ureq::ErrorKind;
    let message = error.to_string();
    let lower = message.to_lowercase();
    let outcome = |variant: &'static str, message: Option<String>| HttpOutcome::Error { variant, url: url.to_string(), message };
    match error.kind() {
        ErrorKind::InvalidUrl | ErrorKind::UnknownScheme => outcome("InvalidUrl", None),
        _ if lower.contains("timed out") || lower.contains("timeout") => outcome("Timeout", None),
        _ if lower.contains("tls") || lower.contains("certificate") => outcome("Tls", Some(message)),
        ErrorKind::TooManyRedirects | ErrorKind::BadStatus | ErrorKind::BadHeader => outcome("InvalidResponse", Some(message)),
        _ => outcome("ConnectionFailed", Some(message)),
    }
}

/// Sends a request (`get`, `post`, `put` or `delete`); a body is sent as JSON when it looks
/// like JSON, otherwise as text.
pub fn http_request(method: &str, url: &str, body: Option<String>) -> HttpOutcome {
    let request = ureq::request(&method.to_uppercase(), url);
    let response = match body {
        Some(body) => {
            let trimmed = body.trim_start();
            let content_type = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            };
            request.set("Content-Type", content_type).send_string(&body)
        }
        None => request.call(),
    };
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(error)) => return transport_error(url, &error),
    };
    let status = response.status() as i64;
    match response.into_string() {
        Ok(body) => HttpOutcome::Response { status, body },
        Err(error) => HttpOutcome::Error {
            variant: "InvalidResponse",
            url: url.to_string(),
            message: Some(error.to_string()),
        },
    }
}

/// Bytes for the library written in Koji, in memory of the C library (which it frees): their address
/// and their size.
fn bytes_for_c(text: &str) -> (u64, u64) {
    let layout = std::alloc::Layout::from_size_align(text.len().max(1), 1).expect("the size of a text");
    let bytes = unsafe { std::alloc::System.alloc(layout) };
    assert!(!bytes.is_null(), "out of memory");
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), bytes, text.len()) };
    (bytes as usize as u64, text.len() as u64)
}

/// HTTPS for std/http.kj (`http://` is in Koji): `method` is 0, 1, 2 or 3 (GET, DELETE, POST,
/// PUT). Gives -1 for a response, with its status in `out[0]`; or else the variant of `HttpError`
/// (0 InvalidUrl, 1 ConnectionFailed, 2 Timeout, 3 Tls, 4 InvalidResponse). The body or the message
/// goes to `out[1]` (an address of `malloc`, or NULL) and `out[2]` (its size).
#[no_mangle]
pub extern "C" fn koji_https_request(method: i64, url: *const KojiString, body: *const KojiString, has_body: bool, out: *mut u64) -> i64 {
    let method = ["get", "delete", "post", "put"][method.clamp(0, 3) as usize];
    let body = has_body.then(|| string_text(body).to_string());
    let out = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    out.copy_from_slice(&[0, 0, 0]);
    match http_request(method, string_text(url), body) {
        HttpOutcome::Response { status, body } => {
            out[0] = status as u64;
            (out[1], out[2]) = bytes_for_c(&body);
            -1
        }
        HttpOutcome::Error { variant, message, .. } => {
            if let Some(message) = message {
                (out[1], out[2]) = bytes_for_c(&message);
            }
            ["InvalidUrl", "ConnectionFailed", "Timeout", "Tls", "InvalidResponse"].iter().position(|v| *v == variant).unwrap_or(4) as i64
        }
    }
}
