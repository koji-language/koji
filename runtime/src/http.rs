//! The `http` module: requests and its functions for compiled
//! programs.

use crate::record::{field_record, make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

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

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

/// `http.get`, `delete`, `post` and `put`: `method` is 0, 1, 2 or 3; `body` is `NULL` for the
/// first two. Gives `Ok(Response)` or `Err(HttpError)`.
#[no_mangle]
pub extern "C" fn koji_std_http_request(
    method: i32,
    url: *const KojiString,
    body: *const KojiString,
    result: *const KojiRecordType,
) -> *mut KojiRecord {
    let method = ["get", "delete", "post", "put"][method.clamp(0, 3) as usize];
    let body = (!body.is_null()).then(|| string_text(body).to_string());
    let _ = std::io::Write::flush(&mut std::io::stdout());
    match http_request(method, string_text(url), body) {
        HttpOutcome::Response { status, body } => {
            let response = make(field_record(result, "Ok", 0), "", &[status as u64, slot(new_string(body))]);
            make(result, "Ok", &[response as usize as u64])
        }
        HttpOutcome::Error { variant, url, message } => {
            let mut fields = vec![slot(new_string(url))];
            if let Some(message) = message {
                fields.push(slot(new_string(message)));
            }
            let error = make(field_record(result, "Err", 0), variant, &fields);
            make(result, "Err", &[error as usize as u64])
        }
    }
}
