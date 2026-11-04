//! The `system` module: command-line arguments, environment variables, the exit code, running
//! other programs and the operating system.

use super::*;
use crate::error::EzError;
use crate::runtime::interpreter::Flow;

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "system.args" => Ok(Value::list(interpreter.args.iter().map(|arg| Value::str(arg)).collect())),
        "system.env" => {
            let name = str_arg(interpreter, &args, 0)?;
            Ok(match std::env::var(name) {
                Ok(value) => some(Value::str(&value)),
                Err(_) => none(),
            })
        }
        "system.exit" => {
            let code = int_arg(interpreter, &args, 0)?;
            let _ = interpreter.out.flush();
            Err(Flow::Error(EzError::Exit(code.clamp(i32::MIN as i64, i32::MAX as i64) as i32)))
        }
        "system.run" | "system.capture" => {
            let command = str_arg(interpreter, &args, 0)?.to_string();
            let arguments: Vec<String> = match args.get(1) {
                Some(Value::List(items)) => items.borrow().iter().map(Value::display).collect(),
                _ => Vec::new(),
            };
            let _ = interpreter.out.flush();
            // `run`: the program uses the terminal (the input and outputs) of this one.
            if name == "system.run" {
                return Ok(match std::process::Command::new(&command).args(&arguments).status() {
                    Ok(status) => ok(Value::Int(status.code().unwrap_or(-1) as i64)),
                    Err(error) => err(system_error(&command, &error)),
                });
            }
            Ok(match std::process::Command::new(&command).args(&arguments).output() {
                Ok(output) => ok(Value::object(vec![
                    ("status".to_string(), Value::Int(output.status.code().unwrap_or(-1) as i64)),
                    ("stdout".to_string(), Value::str(&String::from_utf8_lossy(&output.stdout))),
                    ("stderr".to_string(), Value::str(&String::from_utf8_lossy(&output.stderr))),
                ])),
                Err(error) => err(system_error(&command, &error)),
            })
        }
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}

fn system_error(command: &str, error: &std::io::Error) -> Value {
    use std::io::ErrorKind;
    let command_value = Value::str(command);
    match error.kind() {
        ErrorKind::NotFound => variant("SystemError", "NotFound", vec![command_value]),
        ErrorKind::PermissionDenied => variant("SystemError", "PermissionDenied", vec![command_value]),
        _ => variant("SystemError", "Io", vec![command_value, Value::str(&error.to_string())]),
    }
}
