//! The `http` module.

use super::*;

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "http.get" | "http.delete" => {
            let url = str_arg(interpreter, &args, 0)?.to_string();
            Ok(http_request(&name[5..], &url, None))
        }
        "http.post" | "http.put" => {
            let url = str_arg(interpreter, &args, 0)?.to_string();
            let body = str_arg(interpreter, &args, 1)?.to_string();
            Ok(http_request(&name[5..], &url, Some(body)))
        }
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}

/// Sends a request (shared with the runtime of compiled programs).
fn http_request(method: &str, url: &str, body: Option<String>) -> Value {
    match koji_runtime::http_request(method, url, body) {
        koji_runtime::HttpOutcome::Response { status, body } => ok(Value::object(vec![
            ("status".to_string(), Value::Int(status)),
            ("body".to_string(), Value::str(&body)),
        ])),
        koji_runtime::HttpOutcome::Error { variant: name, url, message } => {
            let mut fields = vec![Value::str(&url)];
            if let Some(message) = message {
                fields.push(Value::str(&message));
            }
            err(variant("HttpError", name, fields))
        }
    }
}
