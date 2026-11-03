//! The `json` module.

use std::fmt::Write as _;

use super::*;
use crate::runtime::value::MapValue;

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "json.parse" => {
            let text = str_arg(interpreter, &args, 0)?;
            Ok(match parse(text) {
                Ok(value) => ok(value),
                Err(error) => err(error),
            })
        }
        "json.stringify" => {
            let mut out = String::new();
            stringify(&args[0], &mut out).map_err(|message| interpreter.error(message))?;
            Ok(Value::str(&out))
        }
        "json.get" => {
            let key = str_arg(interpreter, &args, 1)?;
            Ok(match json_payload(&args[0], "Object") {
                Some(Value::Map(fields)) => match fields.borrow().get(&Value::str(key)) {
                    Some(value) => some(value.clone()),
                    None => none(),
                },
                _ => none(),
            })
        }
        "json.asString" => Ok(json_payload(&args[0], "String").map_or_else(none, some)),
        "json.asInt" => Ok(json_payload(&args[0], "Int").map_or_else(none, some)),
        "json.asBool" => Ok(json_payload(&args[0], "Bool").map_or_else(none, some)),
        "json.asFloat" => Ok(match (json_payload(&args[0], "Float"), json_payload(&args[0], "Int")) {
            (Some(value), _) => some(value),
            (_, Some(Value::Int(n))) => some(Value::Float(n as f64)),
            _ => none(),
        }),
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}

/// The payload of a `Json` value if it is the variant `name` (`String('a')` → `'a'`).
fn json_payload(value: &Value, name: &str) -> Option<Value> {
    match value {
        Value::Variant(v) if v.name == name => v.values.first().cloned(),
        _ => None,
    }
}

fn json(name: &str, values: Vec<Value>) -> Value {
    variant("Json", name, values)
}

/// Parses JSON into a `Json` value; on failure, returns a `JsonError` value.
/// Parses a JSON text into a `Json` value (the parser is shared with the runtime of
/// compiled programs).
pub fn parse(text: &str) -> Result<Value, Value> {
    match koji_runtime::parse_json(text) {
        Ok(tree) => Ok(from_tree(&tree)),
        Err(failure) => Err(variant(
            "JsonError",
            failure.kind,
            vec![Value::str(&failure.text), Value::Int(failure.line), Value::Int(failure.column)],
        )),
    }
}

fn from_tree(tree: &koji_runtime::JsonTree) -> Value {
    use koji_runtime::JsonTree;
    match tree {
        JsonTree::Null => json("Null", vec![]),
        JsonTree::Bool(b) => json("Bool", vec![Value::Bool(*b)]),
        JsonTree::Int(n) => json("Int", vec![Value::Int(*n)]),
        JsonTree::Float(f) => json("Float", vec![Value::Float(*f)]),
        JsonTree::String(s) => json("String", vec![Value::str(s)]),
        JsonTree::Array(items) => json("Array", vec![Value::list(items.iter().map(from_tree).collect())]),
        JsonTree::Object(fields) => {
            let mut map = MapValue::default();
            for (key, value) in fields {
                map.insert(Value::str(key), from_tree(value));
            }
            json("Object", vec![Value::Map(Rc::new(RefCell::new(map)))])
        }
    }
}

use koji_runtime::{write_float, write_string};

pub fn stringify(value: &Value, out: &mut String) -> Result<(), String> {
    let variant = match value {
        Value::Variant(variant) if variant.sum.name == "Json" => variant,
        other => return Err(format!("expected a Json value, found {}", other.type_name())),
    };
    match (variant.name.as_str(), variant.values.first()) {
        ("Null", _) => out.push_str("null"),
        ("Bool", Some(Value::Bool(b))) => {
            let _ = write!(out, "{}", b);
        }
        ("Int", Some(Value::Int(n))) => {
            let _ = write!(out, "{}", n);
        }
        ("Float", Some(Value::Float(f))) => write_float(*f, out),
        ("Float", Some(Value::Int(n))) => {
            let _ = write!(out, "{}.0", n);
        }
        ("String", Some(Value::Str(s))) => write_string(s, out),
        ("Array", Some(Value::List(items))) => {
            out.push('[');
            for (i, item) in items.borrow().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                stringify(item, out)?;
            }
            out.push(']');
        }
        ("Object", Some(Value::Map(object))) => {
            out.push('{');
            for (i, (key, value)) in object.borrow().entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(&key.display(), out);
                out.push(':');
                stringify(value, out)?;
            }
            out.push('}');
        }
        (name, _) => return Err(format!("invalid Json value '{}'", name)),
    }
    Ok(())
}
