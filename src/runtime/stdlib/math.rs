//! The `math` module.

use super::*;

// `x ** y` on real numbers is shared with the runtime of compiled programs.
pub use koji_runtime::{float_pow as real_pow, PowFailure};

fn math_error(name: &str) -> Value {
    err(variant("MathError", name, vec![]))
}

/// Calls the built-in function `name` of this module.
pub(super) fn call(interpreter: &mut Interpreter, name: &str, args: Vec<Value>) -> Exec<Value> {
    match name {
        "math.abs" => match &args[0] {
            Value::Int(n) => n
                .checked_abs()
                .map(Value::Int)
                .ok_or_else(|| interpreter.error("integer overflow")),
            other => Ok(Value::Float(as_float(other).abs())),
        },
        "math.sqrt" => {
            let x = number_arg(interpreter, &args, 0)?;
            if x < 0.0 {
                return Ok(math_error("Complex"));
            }
            Ok(ok(Value::Float(x.sqrt())))
        }
        "math.pow" => {
            let x = number_arg(interpreter, &args, 0)?;
            let y = number_arg(interpreter, &args, 1)?;
            match real_pow(x, y) {
                Ok(result) => Ok(ok(Value::Float(result))),
                Err(PowFailure::Complex) => Ok(math_error("Complex")),
                Err(PowFailure::Infinite) => Ok(math_error("Infinite")),
                Err(PowFailure::Overflow) => Err(interpreter.error("float overflow")),
            }
        }
        "math.floor" | "math.ceil" | "math.round" => {
            let x = number_arg(interpreter, &args, 0)?;
            let rounded = match name {
                "math.floor" => x.floor(),
                "math.ceil" => x.ceil(),
                _ => x.round(),
            };
            float_to_int(interpreter, rounded)
        }
        other => Err(interpreter.error(format!("unknown built-in function '{}'", other))),
    }
}
