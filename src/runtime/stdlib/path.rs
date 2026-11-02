//! The `path` module: working with paths as text (it does not access the disk).

use super::*;
use std::path::Path;

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    let path = Path::new(str_arg(interpreter, &args, 0)?);
    let text = |part: Option<&std::ffi::OsStr>| match part {
        Some(part) => some(Value::str(&part.to_string_lossy())),
        None => none(),
    };
    Ok(match name {
        "path.join" => Value::str(&path.join(str_arg(interpreter, &args, 1)?).to_string_lossy()),
        // The parent of 'main.kj' is '.'; '/' has none.
        "path.parent" => match path.parent() {
            Some(parent) if parent.as_os_str().is_empty() => some(Value::str(".")),
            Some(parent) => some(Value::str(&parent.to_string_lossy())),
            None => none(),
        },
        "path.name" => text(path.file_name()),
        "path.stem" => text(path.file_stem()),
        "path.extension" => text(path.extension()),
        "path.isAbsolute" => Value::Bool(path.is_absolute()),
        other => return Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    })
}
