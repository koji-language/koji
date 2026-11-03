//! Calls: functions, methods, `mut` arguments and `display`.

use super::*;

impl Interpreter {
    pub(super) fn eval_call(
        &mut self,
        callee: &AstNode,
        args: &[AstNode],
        method: Option<MethodRef>,
        displays: &[Option<MethodRef>],
        env: &Rc<Env>,
    ) -> Exec<Value> {
        let mut temporaries = Vec::new();
        let result = self.eval_call_with(callee, args, method, displays, env, &mut temporaries)?;
        // Move-only temporaries used by the call (`show(Conn.open())`) are dropped after it.
        for temporary in temporaries {
            self.drop_value(temporary)?;
        }
        Ok(result)
    }

    fn eval_call_with(
        &mut self,
        callee: &AstNode,
        args: &[AstNode],
        method: Option<MethodRef>,
        displays: &[Option<MethodRef>],
        env: &Rc<Env>,
        temporaries: &mut Vec<Value>,
    ) -> Exec<Value> {
        // A method (`person.greet()`) or an associated function (`Person.create()`).
        if let Some(method) = method {
            // `Set.from(items)`: built in.
            if method.module.as_os_str().is_empty() && method.key == "Set.from" {
                let (values, _, _) = self.eval_args(args, env)?;
                let mut set = MapValue::default();
                if let Some(Value::List(items)) = values.first() {
                    for item in items.borrow().iter() {
                        set.insert(item.clone(), Value::Void);
                    }
                }
                return Ok(Value::Set(Rc::new(RefCell::new(set))));
            }
            let closure = match self.methods.get(&(method.module.clone(), method.key.clone())) {
                Some(closure) => Rc::clone(closure),
                None => return Err(self.error(format!("method '{}' not found", method.key))),
            };
            let mut values = Vec::new();
            let mut places = Vec::new();
            if method.has_self {
                if let AstNode::Member { object, .. } = callee.unlocated() {
                    // `mut self`: the value is stored back where it came from, like a `mut`
                    // argument.
                    if closure.params[0].is_mut {
                        let (value, place) = self.eval_place(object, env)?;
                        values.push(value);
                        places.push((0, place));
                    } else {
                        let value = self.eval(object, env)?;
                        if self.is_temporary(object) {
                            temporaries.push(value.clone());
                        }
                        values.push(value);
                    }
                }
            }
            let offset = values.len();
            let (rest, arg_places, arg_temporaries) = self.eval_args(args, env)?;
            temporaries.extend(arg_temporaries);
            values.extend(rest);
            places.extend(arg_places.into_iter().map(|(i, place)| (i + offset, place)));
            return self.call_with_places(&Value::Function(closure), values, places, env);
        }
        if let AstNode::Member { object: object_node, name } = callee.unlocated() {
            // `items.push(x)`: the list changes where it is (copied first if it is shared).
            if self.ownership.mutating_calls.contains(&node_id(callee)) {
                let place = self.resolve_place(object_node, env)?;
                let (args, _, arg_temporaries) = self.eval_args(args, env)?;
                temporaries.extend(arg_temporaries);
                return self.change_list(&place, name, args, env);
            }
            let object = self.eval(object_node, env)?;
            if self.is_temporary(object_node) {
                temporaries.push(object.clone());
            }
            let (args, places, arg_temporaries) = self.eval_args(args, env)?;
            temporaries.extend(arg_temporaries);
            if !places.is_empty() {
                let function = self.member(&object, name)?;
                return self.call_with_places(&function, args, places, env);
            }
            return self.call_method(&object, name, args);
        }
        let callee = self.eval(callee, env)?;
        let (mut args, places, arg_temporaries) = self.eval_args(args, env)?;
        temporaries.extend(arg_temporaries);
        // `print(person)`: values whose type declares `display` are shown with it.
        for (arg, display) in args.iter_mut().zip(displays) {
            if display.is_some() {
                let value = std::mem::replace(arg, Value::Void);
                *arg = Value::str(&self.display(value, display.as_ref())?);
            }
        }
        self.call_with_places(&callee, args, places, env)
    }

    /// Text of a value for `print` and interpolation: its type's `display` method, if declared.
    pub(super) fn display(&mut self, value: Value, display: Option<&MethodRef>) -> Exec<String> {
        let method = match display {
            Some(method) => method,
            None => return Ok(value.display()),
        };
        let closure = match self.methods.get(&(method.module.clone(), method.key.clone())) {
            Some(closure) => Rc::clone(closure),
            None => return Err(self.error(format!("method '{}' not found", method.key))),
        };
        match self.call_closure(&closure, vec![value])? {
            Value::Str(text) => Ok(text.to_string()),
            other => Ok(other.display()),
        }
    }

    /// The argument values, and where each `mut` argument came from (by argument position).
    pub(super) fn eval_args(&mut self, args: &[AstNode], env: &Rc<Env>) -> Exec<Args> {
        let mut values = Vec::with_capacity(args.len());
        let mut places = Vec::new();
        let mut temporaries = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            match arg.unlocated() {
                AstNode::MutArg(place) => {
                    let (value, place) = self.eval_place(place, env)?;
                    values.push(value);
                    places.push((i, place));
                }
                other => {
                    let value = self.eval(other, env)?;
                    let value = coerce(value, self.ownership.to_float.contains(&node_id(other)));
                    if self.is_temporary(other) {
                        temporaries.push(value.clone());
                    }
                    values.push(value);
                }
            }
        }
        Ok((values, places, temporaries))
    }

    pub(super) fn eval_place(&mut self, node: &AstNode, env: &Rc<Env>) -> Exec<(Value, Place)> {
        let place = self.resolve_place(node, env)?;
        match self.read_place(&place, env)? {
            Some(value) => Ok((value, place)),
            None => Err(self.error("only a variable, a field or an element can be passed as 'mut'")),
        }
    }

    pub(super) fn set_place(&mut self, place: Place, value: Value, env: &Rc<Env>) -> Exec<()> {
        self.write_place(&place, value, env)
    }

    /// Calls `callee`; then stores the final value of each `mut` parameter (by position) back
    /// where its argument came from.
    pub(super) fn call_with_places(
        &mut self,
        callee: &Value,
        args: Vec<Value>,
        places: Vec<(usize, Place)>,
        env: &Rc<Env>,
    ) -> Exec<Value> {
        let closure = match callee {
            Value::Function(closure) => closure,
            other => return self.call_value(other, args),
        };
        // A `mut` argument is the variable itself: the function gets its value at the call,
        // after the other arguments ran (`c.wrap(c.bump())` sees the bumped `c`).
        let mut args = args;
        for (i, place) in &places {
            if let Some(value) = self.read_place(place, env)? {
                args[*i] = value;
            }
        }
        let (value, scope) = self.run_closure(closure, args)?;
        for (i, place) in places {
            let param = &closure.params[i].name;
            if let Some(final_value) = scope.get_local(param) {
                self.set_place(place, final_value, env)?;
            }
        }
        Ok(value)
    }

    pub(crate) fn call_value(&mut self, callee: &Value, args: Vec<Value>) -> Exec<Value> {
        match callee {
            Value::Function(closure) => self.call_closure(closure, args),
            Value::Builtin(name) => stdlib::call(self, name, args),
            Value::Constructor(constructor) => {
                // `Ok()`: the field of type `void`.
                let args = if args.is_empty() && constructor.arity() == 1 { vec![Value::Void] } else { args };
                if args.len() != constructor.arity() {
                    return Err(self.error(format!(
                        "'{}' expects {} argument(s), found {}",
                        constructor.variant,
                        constructor.arity(),
                        args.len()
                    )));
                }
                Ok(constructor.sum.make(&constructor.variant, args))
            }
            other => Err(self.error(format!("a value of type {} cannot be called", other.type_name()))),
        }
    }

    pub(super) fn call_closure(&mut self, closure: &Rc<Closure>, args: Vec<Value>) -> Exec<Value> {
        self.run_closure(closure, args).map(|(value, _)| value)
    }

    /// Runs a function; also returns the scope of its parameters.
    pub(super) fn run_closure(&mut self, closure: &Rc<Closure>, args: Vec<Value>) -> Exec<(Value, Rc<Env>)> {
        if args.len() != closure.params.len() {
            return Err(self.error(format!(
                "'{}' expects {} argument(s), found {}",
                closure.name,
                closure.params.len(),
                args.len()
            )));
        }
        let scope = Env::child(&closure.env);
        for ((param, value), to_float) in closure.params.iter().zip(args).zip(&closure.float_params) {
            scope.define(&param.name, coerce(value, *to_float));
            // A `take` parameter owns its value: it is dropped when the function returns.
            if param.is_take {
                scope.own(&param.name);
            }
        }

        let saved_file = std::mem::replace(&mut self.file, Rc::clone(&closure.file));
        let saved_line = self.line;
        self.function_depth += 1;
        let result = match closure.body.unlocated() {
            AstNode::Block(statements) => self.exec_statements(statements, &scope),
            expression => self.eval(expression, &scope),
        };
        // The function's own variables are dropped when it returns.
        let result = self.end_scope(&scope, result);
        self.function_depth -= 1;
        self.file = saved_file;
        self.line = saved_line;

        let value = match result {
            Ok(value) | Err(Flow::Return(value)) => value,
            Err(Flow::Break) | Err(Flow::Continue) => Value::Void,
            Err(error) => return Err(error),
        };
        Ok((coerce(value, closure.returns_float), scope))
    }

    pub(super) fn call_method(&mut self, object: &Value, name: &str, args: Vec<Value>) -> Exec<Value> {
        // Every value has `display()` and `debug()`, unless a field of an object has the name.
        let is_field = matches!(object, Value::Object(fields) if fields.borrow().get(name).is_some());
        if !is_field && !matches!(object, Value::Module(_)) {
            match name {
                "display" => return Ok(Value::str(&object.display())),
                "debug" => return Ok(Value::str(&object.repr())),
                _ => {}
            }
        }
        match object {
            Value::Module(_) => {
                let member = self.member(object, name)?;
                self.call_value(&member, args)
            }
            Value::Str(s) => stdlib::string_method(self, s, name, args),
            Value::Char(c) if name == "code" => Ok(Value::Int(*c as i64)),
            Value::List(items) => match name {
                "length" => Ok(Value::Int(items.borrow().len() as i64)),
                "indexOf" => {
                    let wanted = args.first().cloned().unwrap_or(Value::Void);
                    Ok(match items.borrow().iter().position(|item| item.equals(&wanted)) {
                        Some(i) => some(Value::Int(i as i64)),
                        None => stdlib::none(),
                    })
                }
                "join" => {
                    let separator = match args.first() {
                        Some(Value::Str(separator)) => separator.to_string(),
                        _ => return Err(self.error("'join' needs a separator string")),
                    };
                    let parts: Vec<String> = items.borrow().iter().map(Value::display).collect();
                    Ok(Value::str(&parts.join(&separator)))
                }
                _ => Err(self.error(format!("lists have no method '{}'", name))),
            },
            Value::Object(fields) => {
                if name == "length" && fields.borrow().get("length").is_none() {
                    return Ok(Value::Int(fields.borrow().fields.len() as i64));
                }
                let member = self.member(object, name)?;
                self.call_value(&member, args)
            }
            Value::Native(native) => stdlib::native_method(self, native, name, args),
            Value::Map(map) | Value::Set(map) => {
                let is_map = matches!(object, Value::Map(_));
                let map = map.borrow();
                let key = args.first();
                Ok(match name {
                    "get" => match key.and_then(|key| map.get(key)) {
                        Some(value) => some(value.clone()),
                        None => stdlib::none(),
                    },
                    "has" => Value::Bool(key.is_some_and(|key| map.get(key).is_some())),
                    "keys" => Value::list(map.entries.iter().map(|(key, _)| key.clone()).collect()),
                    "values" if is_map => Value::list(map.entries.iter().map(|(_, value)| value.clone()).collect()),
                    "values" => Value::list(map.entries.iter().map(|(item, _)| item.clone()).collect()),
                    "length" => Value::Int(map.len() as i64),
                    other => return Err(self.error(format!("{} has no method '{}'", object.type_name(), other))),
                })
            }
            Value::Variant(variant) if variant.sum.name == "Result" && name == "mapErr" => {
                if variant.name != "Err" {
                    return Ok(object.clone());
                }
                let converter = args.into_iter().next().unwrap_or(Value::Void);
                let error = self.call_value(&converter, vec![variant.values[0].clone()])?;
                Ok(stdlib::err(error))
            }
            other => Err(self.error(format!(
                "a value of type {} has no method '{}'",
                other.type_name(),
                name
            ))),
        }
    }

    /// The built-in methods that change the value they are called on: `push`, `pop`, `insert`,
    /// `removeAt`, `sort` and `sortBy` on lists, `remove` on maps, `add` and `remove` on sets.
    fn change_list(&mut self, place: &Place, name: &str, args: Vec<Value>, env: &Rc<Env>) -> Exec<Value> {
        if name == "sort" || name == "sortBy" {
            let items = match self.read_place(place, env)? {
                Some(Value::List(items)) => items.borrow().clone(),
                _ => return Err(self.error(format!("'{}' is only for lists", name))),
            };
            let sorted = match args.first() {
                Some(comparison) => self.merge_sort(items, comparison)?,
                None => {
                    let mut items = items;
                    items.sort_by(compare_values);
                    items
                }
            };
            self.write_place(place, Value::list(sorted), env)?;
            return Ok(Value::Void);
        }
        let mut args = args.into_iter();
        let (first, second) = (args.next(), args.next());
        let position = |value: &Option<Value>, length: usize, allow_end: bool| -> Result<usize, String> {
            match value {
                Some(Value::Int(i)) if *i >= 0 && ((*i as usize) < length || (allow_end && *i as usize == length)) => {
                    Ok(*i as usize)
                }
                Some(Value::Int(i)) => Err(format!("index {} out of range (length {})", i, length)),
                _ => Err("the position must be an int".to_string()),
            }
        };
        self.modify_place(place, env, |slot| {
            let is_map = matches!(slot, Value::Map(_));
            let items = match slot {
                Value::List(items) => items,
                // `map.remove(key)`, `set.add(item)`, `set.remove(item)`.
                Value::Map(map) | Value::Set(map) => {
                    if Rc::strong_count(map) > 1 {
                        let copy = map.borrow().clone();
                        *map = Rc::new(RefCell::new(copy));
                    }
                    let mut map = map.borrow_mut();
                    let key = first.unwrap_or(Value::Void);
                    return Ok(match (name, is_map) {
                        ("remove", true) => match map.remove(&key) {
                            Some(value) => some(value),
                            None => stdlib::none(),
                        },
                        ("remove", false) => Value::Bool(map.remove(&key).is_some()),
                        ("add", false) => {
                            map.insert(key, Value::Void);
                            Value::Void
                        }
                        (other, _) => return Err(format!("no method '{}'", other)),
                    });
                }
                _ => return Err(format!("'{}' is only for lists", name)),
            };
            if Rc::strong_count(items) > 1 {
                let copy = items.borrow().clone();
                *items = Rc::new(RefCell::new(copy));
            }
            let mut items = items.borrow_mut();
            Ok(match name {
                "push" => {
                    items.push(first.unwrap_or(Value::Void));
                    Value::Void
                }
                "insert" => {
                    let at = position(&first, items.len(), true)?;
                    items.insert(at, second.unwrap_or(Value::Void));
                    Value::Void
                }
                "pop" => match items.pop() {
                    Some(item) => some(item),
                    None => stdlib::none(),
                },
                "removeAt" => match position(&first, items.len(), false) {
                    Ok(at) => some(items.remove(at)),
                    Err(_) => stdlib::none(),
                },
                other => return Err(format!("lists have no method '{}'", other)),
            })
        })
    }

    /// A stable sort with `comparison(a, b)`: whether `a` goes before `b`.
    fn merge_sort(&mut self, items: Vec<Value>, comparison: &Value) -> Exec<Vec<Value>> {
        if items.len() <= 1 {
            return Ok(items);
        }
        let mut left = items;
        let right = left.split_off(left.len() / 2);
        let left = self.merge_sort(left, comparison)?;
        let right = self.merge_sort(right, comparison)?;
        let mut merged = Vec::with_capacity(left.len() + right.len());
        let (mut left, mut right) = (left.into_iter().peekable(), right.into_iter().peekable());
        while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
            // Take from the right only when it goes strictly before: equal items keep their order.
            let right_first = matches!(self.call_value(comparison, vec![b.clone(), a.clone()])?, Value::Bool(true));
            merged.push(if right_first { right.next() } else { left.next() }.expect("peeked"));
        }
        merged.extend(left);
        merged.extend(right);
        Ok(merged)
    }
}

/// The order of `int`, `float`, `string` and `char` values (`sort`).
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Char(x), Value::Char(y)) => x.cmp(y),
        _ => as_float(a).partial_cmp(&as_float(b)).unwrap_or(std::cmp::Ordering::Equal),
    }
}
