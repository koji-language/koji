//! The `datetime` module (UTC).

use super::*;
pub use koji_runtime::datetime::{datetime_from_timestamp, format_datetime, parse_datetime, DateFailure};

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "datetime.now" => Ok(Value::Native(Rc::new(RefCell::new(Native::DateTime(
            datetime_from_timestamp(unix_now().floor() as i64),
        ))))),
        "datetime.today" => {
            let now = datetime_from_timestamp(unix_now().floor() as i64);
            Ok(Value::str(&format!("{:04}-{:02}-{:02}", now.year, now.month, now.day)))
        }
        "datetime.format" => {
            let timestamp = number_arg(interpreter, &args, 0)?;
            let pattern = str_arg(interpreter, &args, 1)?;
            let datetime = datetime_from_timestamp(timestamp.floor() as i64);
            Ok(Value::str(&format_datetime(&datetime, pattern)))
        }
        "datetime.parse" => {
            let text = str_arg(interpreter, &args, 0)?;
            let pattern = str_arg(interpreter, &args, 1)?;
            Ok(match parse_datetime(text, pattern) {
                Ok(timestamp) => ok(Value::Int(timestamp)),
                Err(DateFailure::PatternMismatch) => err(variant(
                    "DateError",
                    "PatternMismatch",
                    vec![Value::str(text), Value::str(pattern)],
                )),
                Err(DateFailure::InvalidDate) => {
                    err(variant("DateError", "InvalidDate", vec![Value::str(text)]))
                }
            })
        }
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}
