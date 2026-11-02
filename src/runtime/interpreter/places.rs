//! Places (`items[0].name`): reading them, and changing them with copy-on-write.
//!
//! Values have value semantics: assigning or passing a list or an object gives an independent
//! copy. Copies share their memory until one of them changes: before a change, every shared
//! level on the way to the changed value is copied, so the other copies keep their contents.

use super::*;
use crate::runtime::value::Object;

impl Interpreter {
    /// The place written as `node`. Indices are evaluated here, from left to right.
    pub(super) fn resolve_place(&mut self, node: &AstNode, env: &Rc<Env>) -> Exec<Place> {
        match node.unlocated() {
            AstNode::Identifier(name) => Ok(Place {
                base: Base::Variable(name.clone()),
                steps: Vec::new(),
            }),
            AstNode::Member { object, name } => {
                let mut place = self.resolve_place(object, env)?;
                place.steps.push(Step::Field(name.clone()));
                Ok(place)
            }
            AstNode::Index { target, index } => {
                let mut place = self.resolve_place(target, env)?;
                let index = self.eval(index, env)?;
                place.steps.push(Step::Index(index));
                Ok(place)
            }
            other => Ok(Place {
                base: Base::Temporary(self.eval(other, env)?),
                steps: Vec::new(),
            }),
        }
    }

    /// The value at `place`. A key missing from a dictionary at the last step gives `None`.
    pub(super) fn read_place(&self, place: &Place, env: &Rc<Env>) -> Exec<Option<Value>> {
        let mut value = match &place.base {
            Base::Variable(name) => match env.get(name) {
                Some(value) => value,
                None => return Ok(None),
            },
            Base::Temporary(value) => value.clone(),
        };
        for (i, step) in place.steps.iter().enumerate() {
            let is_last = i + 1 == place.steps.len();
            value = match (step, &value, step.key()) {
                (_, Value::Object(object), Some(key)) if is_last => match object.borrow().get(key) {
                    Some(value) => value.clone(),
                    None => return Ok(None),
                },
                (Step::Field(name), _, _) => self.member(&value, name)?,
                (Step::Index(index), _, _) => self.index(&value, index)?,
            };
        }
        Ok(Some(value))
    }

    /// Stores `value` at `place`, copying the shared levels on the way.
    pub(super) fn write_place(&self, place: &Place, value: Value, env: &Rc<Env>) -> Exec<()> {
        self.modify_place(place, env, move |slot| {
            *slot = value;
            Ok(())
        })
    }

    /// Runs `f` on the value at `place` itself, copying the shared levels on the way (so `f`
    /// can change it in place: `items.push(x)`).
    pub(super) fn modify_place<R>(
        &self,
        place: &Place,
        env: &Rc<Env>,
        f: impl FnOnce(&mut Value) -> Result<R, String>,
    ) -> Exec<R> {
        let result = match &place.base {
            Base::Variable(name) => match env.with_slot(name, |slot| apply_in(slot, &place.steps, f)) {
                Some(result) => result,
                None => Err(format!("undefined variable '{}'", name)),
            },
            // A temporary value is discarded after the change.
            Base::Temporary(temporary) => apply_in(&mut temporary.clone(), &place.steps, f),
        };
        result.map_err(|message| self.error(message))
    }
}

/// Makes `items` the only owner of its list, copying it if it is shared.
fn unshare_list(items: &mut Rc<RefCell<Vec<Value>>>) {
    if Rc::strong_count(items) > 1 {
        let copy = items.borrow().clone();
        *items = Rc::new(RefCell::new(copy));
    }
}

fn unshare_object(object: &mut Rc<RefCell<Object>>) {
    if Rc::strong_count(object) > 1 {
        let copy = object.borrow().clone();
        *object = Rc::new(RefCell::new(copy));
    }
}

/// Runs `f` on the value at the end of `steps`, starting at `slot`.
fn apply_in<R>(slot: &mut Value, steps: &[Step], f: impl FnOnce(&mut Value) -> Result<R, String>) -> Result<R, String> {
    let (step, rest) = match steps.split_first() {
        Some(split) => split,
        None => return f(slot),
    };
    match (slot, step) {
        (Value::List(items), Step::Index(index)) => {
            unshare_list(items);
            let mut items = items.borrow_mut();
            let position = match index {
                Value::Int(i) if *i >= 0 && (*i as usize) < items.len() => *i as usize,
                Value::Int(i) => return Err(format!("index {} out of range (length {})", i, items.len())),
                other => return Err(format!("index must be int, found {}", other.type_name())),
            };
            apply_in(&mut items[position], rest, f)
        }
        (Value::Object(object), step) if step.key().is_some() => {
            let key = step.key().unwrap_or_default();
            unshare_object(object);
            let mut object = object.borrow_mut();
            if object.get(key).is_none() && rest.is_empty() {
                // `dictionary['new'] = value` adds the key.
                object.set(key, Value::Void);
            }
            match object.get_mut(key) {
                Some(field) => apply_in(field, rest, f),
                None => Err(format!("key '{}' not found", key)),
            }
        }
        (Value::Object(_), Step::Index(other)) => {
            Err(format!("dictionary keys must be strings, found {}", other.type_name()))
        }
        (other, Step::Index(_)) => Err(format!("cannot assign by index to a value of type {}", other.type_name())),
        (other, Step::Field(name)) => Err(format!(
            "cannot assign field '{}' on a value of type {}",
            name,
            other.type_name()
        )),
    }
}
