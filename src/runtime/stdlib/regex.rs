//! The `regex` module and the methods of `Regex`.

use super::*;

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "regex.compile" => {
            let pattern = str_arg(interpreter, &args, 0)?;
            Ok(match ::regex::Regex::new(pattern) {
                Ok(regex) => ok(Value::Native(Rc::new(RefCell::new(Native::Regex(regex))))),
                Err(error) => err(regex_error(pattern, &error)),
            })
        }
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}

/// The last line of the `regex` crate's message (`unclosed group`), without the drawing of the
/// pattern above it.
use koji_runtime::regex_error_message;

/// `TooLarge` for a pattern too large to compile, `Unsupported` for valid syntax that the
/// engine does not support (look-around, backreferences), `InvalidPattern` otherwise.
fn regex_error(pattern: &str, error: &::regex::Error) -> Value {
    let pattern_value = Value::str(pattern);
    match error {
        ::regex::Error::CompiledTooBig(_) => variant("RegexError", "TooLarge", vec![pattern_value]),
        _ => {
            let message = regex_error_message(error);
            let name = if message.contains("not supported") { "Unsupported" } else { "InvalidPattern" };
            variant("RegexError", name, vec![pattern_value, Value::str(&message)])
        }
    }
}

/// Methods of a compiled `Regex`.
pub(super) fn method(interpreter: &mut Interpreter, regex: &::regex::Regex, name: &str, args: Vec<Value>) -> Exec<Value> {
    let input = str_arg(interpreter, &args, 0)?;
    match name {
        "test" => Ok(Value::Bool(regex.is_match(input))),
        "find" => Ok(match regex.find(input) {
            Some(found) => some(match_object(input, &found)),
            None => none(),
        }),
        "findAll" => Ok(Value::list(
            regex.find_iter(input).map(|m| match_object(input, &m)).collect(),
        )),
        "replace" | "replaceAll" => {
            let replacement = str_arg(interpreter, &args, 1)?;
            Ok(Value::str(&if name == "replace" {
                regex.replace(input, replacement)
            } else {
                regex.replace_all(input, replacement)
            }))
        }
        "split" => Ok(Value::list(regex.split(input).map(Value::str).collect())),
        other => Err(interpreter.error(format!("Regex has no method '{}'", other))),
    }
}

/// `Match { value, start, end }`, with positions counted in characters.
fn match_object(input: &str, found: &::regex::Match) -> Value {
    let start = input[..found.start()].chars().count() as i64;
    let end = start + found.as_str().chars().count() as i64;
    Value::object(vec![
        ("value".to_string(), Value::str(found.as_str())),
        ("start".to_string(), Value::Int(start)),
        ("end".to_string(), Value::Int(end)),
    ])
}
