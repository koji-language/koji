use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;

use crate::parser::ast::{AstNode, Param};
use crate::runtime::env::Env;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    /// Strings have value semantics too: `+=` appends in place when the string is not shared.
    Str(Rc<String>),
    Char(char),
    Bool(bool),
    Void,
    /// Lists and objects have value semantics: copies share their memory until one of them
    /// changes (copy-on-write, see `interpreter/places.rs`).
    List(Rc<RefCell<Vec<Value>>>),
    /// Shape-typed values, object literals and dictionaries.
    Object(Rc<RefCell<Object>>),
    Variant(Rc<VariantValue>),
    /// `Map[K, V]` and `Set[T]` (a set has `void` values), with value semantics like lists.
    Map(Rc<RefCell<MapValue>>),
    Set(Rc<RefCell<MapValue>>),
    Function(Rc<Closure>),
    /// Built-in function, by qualified name (`print`, `math.sqrt`).
    Builtin(Rc<str>),
    Constructor(Rc<Constructor>),
    Module(Rc<ModuleValue>),
    Native(Rc<RefCell<Native>>),
}

#[derive(Debug, Default, Clone)]
pub struct Object {
    /// Fields in declaration order.
    pub fields: Vec<(String, Value)>,
    /// For a type with a `drop` method: that method (module, `Type.drop`).
    pub drop: Option<Rc<(PathBuf, String)>>,
}

impl Object {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.fields.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn set(&mut self, name: &str, value: Value) {
        match self.fields.iter_mut().find(|(n, _)| n == name) {
            Some((_, slot)) => *slot = value,
            None => self.fields.push((name.to_string(), value)),
        }
    }
}

/// A sum type known at runtime: its name and its variants, with their field names.
#[derive(Debug)]
pub struct SumType {
    pub name: String,
    /// Module that declares the type (empty for built-in types), where its methods are.
    pub module: PathBuf,
    pub variants: Vec<(String, Vec<String>)>,
}

impl SumType {
    pub fn has_variant(&self, variant: &str) -> bool {
        self.variants.iter().any(|(name, _)| name == variant)
    }

    pub fn fields(&self, variant: &str) -> &[String] {
        self.variants
            .iter()
            .find(|(name, _)| name == variant)
            .map(|(_, fields)| fields.as_slice())
            .unwrap_or(&[])
    }

    /// The value bound to a variant name: the value itself for a variant without fields,
    /// a constructor otherwise.
    pub fn constructor(self: &Rc<Self>, variant: &str) -> Value {
        if self.fields(variant).is_empty() {
            self.make(variant, vec![])
        } else {
            Value::Constructor(Rc::new(Constructor {
                sum: Rc::clone(self),
                variant: variant.to_string(),
            }))
        }
    }

    pub fn make(self: &Rc<Self>, variant: &str, values: Vec<Value>) -> Value {
        Value::Variant(Rc::new(VariantValue {
            sum: Rc::clone(self),
            name: variant.to_string(),
            values,
        }))
    }
}

#[derive(Debug)]
pub struct VariantValue {
    pub sum: Rc<SumType>,
    pub name: String,
    pub values: Vec<Value>,
}

impl VariantValue {
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.sum
            .fields(&self.name)
            .iter()
            .position(|n| n == name)
            .map(|i| &self.values[i])
    }
}

#[derive(Debug)]
pub struct Closure {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Rc<AstNode>,
    pub env: Rc<Env>,
    /// Parameters declared as `float` (integers passed to them are converted).
    pub float_params: Vec<bool>,
    pub returns_float: bool,
    /// File where the function was defined (for error messages).
    pub file: Rc<str>,
}

#[derive(Debug)]
pub struct Constructor {
    pub sum: Rc<SumType>,
    pub variant: String,
}

impl Constructor {
    pub fn arity(&self) -> usize {
        self.sum.fields(&self.variant).len()
    }
}

#[derive(Debug)]
pub struct ModuleValue {
    pub name: String,
    pub env: Rc<Env>,
    /// Sum types declared at the top level of the module.
    pub types: RefCell<Vec<Rc<SumType>>>,
}

#[derive(Debug)]
pub enum Native {
    File(FileHandle),
    DateTime(DateTime),
    Regex(regex::Regex),
}

#[derive(Debug)]
pub struct FileHandle {
    pub path: String,
    pub mode: String,
    pub file: Option<std::fs::File>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

impl DateTime {
    pub fn field(&self, name: &str) -> Option<i64> {
        Some(match name {
            "year" => self.year,
            "month" => self.month,
            "day" => self.day,
            "hour" => self.hour,
            "minute" => self.minute,
            "second" => self.second,
            _ => return None,
        })
    }
}

impl std::fmt::Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// The text of a float, shared with the runtime of compiled programs.
pub use koji_runtime::format_float;

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::new(s.to_string()))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn object(fields: Vec<(String, Value)>) -> Value {
        Value::Object(Rc::new(RefCell::new(Object { fields, drop: None })))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Bool(_) => "boolean",
            Value::Char(_) => "char",
            Value::Void => "void",
            Value::List(_) => "list",
            Value::Object(_) => "object",
            Value::Map(_) => "Map",
            Value::Set(_) => "Set",
            Value::Variant(_) => "variant",
            Value::Function(_) | Value::Builtin(_) | Value::Constructor(_) => "function",
            Value::Module(_) => "module",
            Value::Native(native) => match &*native.borrow() {
                Native::File(_) => "File",
                Native::DateTime(_) => "DateTime",
                Native::Regex(_) => "Regex",
            },
        }
    }

    /// Text shown by `print` and string interpolation. Standard library errors show a
    /// human-readable message.
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            Value::Char(c) => c.to_string(),
            Value::Variant(variant) => crate::runtime::stdlib::error_message(variant)
                .unwrap_or_else(|| self.repr()),
            other => other.repr(),
        }
    }

    /// Text of a value nested in another one (strings are quoted).
    pub fn repr(&self) -> String {
        let mut out = String::new();
        self.write_repr(&mut out);
        out
    }

    fn write_repr(&self, out: &mut String) {
        match self {
            Value::Int(n) => {
                let _ = write!(out, "{}", n);
            }
            Value::Float(f) => out.push_str(&format_float(*f)),
            Value::Str(s) => {
                out.push('\'');
                for c in s.chars() {
                    match c {
                        '\'' => out.push_str("\\'"),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        '\r' => out.push_str("\\r"),
                        other => out.push(other),
                    }
                }
                out.push('\'');
            }
            Value::Char(c) => {
                out.push('\'');
                match c {
                    '\'' => out.push_str("\\'"),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    other => out.push(*other),
                }
                out.push('\'');
            }
            Value::Bool(b) => {
                let _ = write!(out, "{}", b);
            }
            Value::Void => out.push_str("void"),
            Value::List(items) => {
                out.push('[');
                for (i, item) in items.borrow().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_repr(out);
                }
                out.push(']');
            }
            Value::Object(object) => {
                let object = object.borrow();
                if object.fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{ ");
                for (i, (name, value)) in object.fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if is_identifier(name) {
                        out.push_str(name);
                    } else {
                        Value::str(name).write_repr(out);
                    }
                    out.push_str(": ");
                    value.write_repr(out);
                }
                out.push_str(" }");
            }
            Value::Map(map) => {
                let map = map.borrow();
                if map.entries.is_empty() {
                    out.push_str("[:]");
                    return;
                }
                out.push('[');
                for (i, (key, value)) in map.entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    key.write_repr(out);
                    out.push_str(": ");
                    value.write_repr(out);
                }
                out.push(']');
            }
            Value::Set(set) => {
                out.push_str("Set([");
                for (i, (item, _)) in set.borrow().entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_repr(out);
                }
                out.push_str("])");
            }
            Value::Variant(variant) => {
                out.push_str(&variant.name);
                if !variant.values.is_empty() {
                    out.push('(');
                    for (i, value) in variant.values.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        value.write_repr(out);
                    }
                    out.push(')');
                }
            }
            Value::Function(closure) => {
                let _ = write!(out, "<function {}>", closure.name);
            }
            Value::Builtin(name) => {
                let _ = write!(out, "<built-in function {}>", name);
            }
            Value::Constructor(constructor) => {
                let _ = write!(out, "<constructor {}>", constructor.variant);
            }
            Value::Module(module) => {
                let _ = write!(out, "<module {}>", module.name);
            }
            Value::Native(native) => match &*native.borrow() {
                Native::File(file) => {
                    let _ = write!(out, "<file '{}'>", file.path);
                }
                Native::DateTime(datetime) => {
                    let _ = write!(out, "{}", datetime);
                }
                Native::Regex(regex) => {
                    let _ = write!(out, "<regex '{}'>", regex.as_str());
                }
            },
        }
    }

    /// Structural equality (`==`).
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => *a as f64 == *b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Void, Value::Void) => true,
            (Value::List(a), Value::List(b)) => {
                Rc::ptr_eq(a, b) || {
                    let (a, b) = (a.borrow(), b.borrow());
                    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
                }
            }
            (Value::Map(a), Value::Map(b)) | (Value::Set(a), Value::Set(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len()
                    && a.entries.iter().all(|(key, value)| b.get(key).is_some_and(|other| value.equals(other)))
            }
            (Value::Object(a), Value::Object(b)) => {
                Rc::ptr_eq(a, b) || {
                    let (a, b) = (a.borrow(), b.borrow());
                    a.fields.len() == b.fields.len()
                        && a
                            .fields
                            .iter()
                            .all(|(name, value)| b.get(name).is_some_and(|v| value.equals(v)))
                }
            }
            (Value::Variant(a), Value::Variant(b)) => {
                a.name == b.name
                    && a.values.len() == b.values.len()
                    && a.values.iter().zip(&b.values).all(|(x, y)| x.equals(y))
            }
            (Value::Native(a), Value::Native(b)) => {
                Rc::ptr_eq(a, b)
                    || match (&*a.borrow(), &*b.borrow()) {
                        (Native::DateTime(x), Native::DateTime(y)) => x == y,
                        _ => false,
                    }
            }
            _ => self.identical(other),
        }
    }

    /// Equality used by `equals` for values that are not compared by their contents
    /// (functions, modules, files...).
    pub fn identical(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Variant(a), Value::Variant(b)) => {
                Rc::ptr_eq(a, b) || (a.values.is_empty() && b.values.is_empty() && a.name == b.name)
            }
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::Constructor(a), Value::Constructor(b)) => Rc::ptr_eq(a, b),
            (Value::Module(a), Value::Module(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => Rc::ptr_eq(a, b),
            (Value::List(_) | Value::Object(_) | Value::Variant(_), _)
            | (_, Value::List(_) | Value::Object(_) | Value::Variant(_)) => false,
            _ => self.equals(other),
        }
    }
}

/// The entries of a map (or the items of a set), in insertion order, with an index by the
/// hash of their key.
#[derive(Debug, Clone, Default)]
pub struct MapValue {
    pub entries: Vec<(Value, Value)>,
    index: std::collections::HashMap<u64, Vec<usize>>,
}

impl MapValue {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn position(&self, key: &Value) -> Option<usize> {
        self.index
            .get(&hash_value(key))?
            .iter()
            .copied()
            .find(|&i| self.entries[i].0.equals(key))
    }

    pub fn get(&self, key: &Value) -> Option<&Value> {
        self.position(key).map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &Value) -> Option<&mut Value> {
        self.position(key).map(|i| &mut self.entries[i].1)
    }

    /// Adds or replaces the value of `key`; returns the old value.
    pub fn insert(&mut self, key: Value, value: Value) -> Option<Value> {
        match self.position(&key) {
            Some(i) => Some(std::mem::replace(&mut self.entries[i].1, value)),
            None => {
                self.index.entry(hash_value(&key)).or_default().push(self.entries.len());
                self.entries.push((key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &Value) -> Option<Value> {
        let i = self.position(key)?;
        let (_, value) = self.entries.remove(i);
        // The positions after `i` moved: rebuild the index.
        self.index.clear();
        for (i, (key, _)) in self.entries.iter().enumerate() {
            self.index.entry(hash_value(key)).or_default().push(i);
        }
        Some(value)
    }
}

/// A hash that agrees with `Value::equals` (fields and entries in any order hash the same).
fn hash_value(value: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    fn feed(value: &Value, hasher: &mut std::collections::hash_map::DefaultHasher) {
        let unordered = |items: Vec<u64>, hasher: &mut std::collections::hash_map::DefaultHasher| {
            items.into_iter().fold(0u64, u64::wrapping_add).hash(hasher)
        };
        match value {
            Value::Int(n) => (0u8, *n).hash(hasher),
            // A float with an integer value hashes like the int (`==` treats them as equal).
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 9.2e18 => (0u8, *f as i64).hash(hasher),
            Value::Float(f) => (1u8, f.to_bits()).hash(hasher),
            Value::Str(s) => (2u8, s.as_str()).hash(hasher),
            Value::Char(c) => (3u8, *c).hash(hasher),
            Value::Bool(b) => (4u8, *b).hash(hasher),
            Value::List(items) => {
                5u8.hash(hasher);
                for item in items.borrow().iter() {
                    feed(item, hasher);
                }
            }
            Value::Object(object) => {
                6u8.hash(hasher);
                let object = object.borrow();
                let parts = object.fields.iter().map(|(name, value)| {
                    let mut part = std::collections::hash_map::DefaultHasher::new();
                    name.hash(&mut part);
                    feed(value, &mut part);
                    part.finish()
                });
                unordered(parts.collect(), hasher);
            }
            Value::Variant(variant) => {
                (7u8, variant.name.as_str()).hash(hasher);
                for value in &variant.values {
                    feed(value, hasher);
                }
            }
            Value::Map(map) | Value::Set(map) => {
                8u8.hash(hasher);
                let map = map.borrow();
                let parts = map.entries.iter().map(|(key, value)| {
                    let mut part = std::collections::hash_map::DefaultHasher::new();
                    feed(key, &mut part);
                    feed(value, &mut part);
                    part.finish()
                });
                unordered(parts.collect(), hasher);
            }
            _ => 9u8.hash(hasher),
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    feed(value, &mut hasher);
    hasher.finish()
}
