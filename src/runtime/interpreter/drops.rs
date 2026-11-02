//! Dropping move-only values: when their owner goes out of scope, when they are replaced, and
//! when a temporary has been used. Dropping a value runs the `drop` method of its type (if it
//! has one), then drops what the value holds; a `File` is closed.

use super::*;
use crate::runtime::value::Native;

impl Interpreter {
    /// Whether the checker marked `node` (see `Ownership`).
    pub(super) fn is_temporary(&self, node: &AstNode) -> bool {
        !self.ownership.temporaries.is_empty() && self.ownership.temporaries.contains(&node_id(node))
    }

    /// Drops the values owned by the variables of `env`, last declared first.
    pub(super) fn drop_scope(&mut self, env: &Rc<Env>) -> Exec<()> {
        for value in env.take_owned() {
            self.drop_value(value)?;
        }
        Ok(())
    }

    /// Ends a scope: drops what it owns, unless the program is stopping with an error.
    pub(super) fn end_scope<T>(&mut self, env: &Rc<Env>, result: Exec<T>) -> Exec<T> {
        if matches!(result, Err(Flow::Error(_))) {
            return result;
        }
        self.drop_scope(env)?;
        result
    }

    pub(super) fn drop_value(&mut self, value: Value) -> Exec<()> {
        let method = match &value {
            Value::Object(object) => object.borrow().drop.as_deref().cloned(),
            Value::Variant(variant) if !variant.sum.module.as_os_str().is_empty() => {
                Some((variant.sum.module.clone(), format!("{}.drop", variant.sum.name)))
            }
            _ => None,
        };
        let mut value = value;
        if let Some(closure) = method.and_then(|key| self.methods.get(&key).cloned()) {
            // `drop(mut self)` may change the value before what it holds is dropped.
            let (_, scope) = self.run_closure(&closure, vec![value.clone()])?;
            if let Some(changed) = closure.params.first().and_then(|self_param| scope.get_local(&self_param.name)) {
                value = changed;
            }
        }
        match value {
            Value::Object(object) => {
                let fields: Vec<Value> = object.borrow().fields.iter().map(|(_, value)| value.clone()).collect();
                for field in fields {
                    self.drop_value(field)?;
                }
            }
            Value::Variant(variant) => {
                for field in variant.values.clone() {
                    self.drop_value(field)?;
                }
            }
            Value::List(items) => {
                let items = items.borrow().clone();
                for item in items {
                    self.drop_value(item)?;
                }
            }
            Value::Map(map) | Value::Set(map) => {
                let entries = map.borrow().entries.clone();
                for (key, value) in entries {
                    self.drop_value(key)?;
                    self.drop_value(value)?;
                }
            }
            Value::Native(native) => {
                if let Native::File(handle) = &mut *native.borrow_mut() {
                    handle.file = None;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
