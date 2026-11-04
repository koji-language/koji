//! JSON: the parser and the writing of text, and the `json`
//! module for compiled programs.

use std::fmt::Write as _;

use crate::list::{new_list, KojiElementType, KojiList};
use crate::map::{koji_map_new, koji_map_set, map_entries, KojiMap};
use crate::record::{field_record, make, record_slots, record_variant, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonTree {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<JsonTree>),
    /// The fields in the order they first appear.
    Object(Vec<(String, JsonTree)>),
}

/// Why a text is not valid JSON: the variant of `JsonError` (`Syntax` or
/// `NumberOutOfRange`), its text, and the line and column where the problem is.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonFailure {
    pub kind: &'static str,
    pub text: String,
    pub line: i64,
    pub column: i64,
}

pub(crate) enum Failure {
    Syntax(String, usize),
    NumberOutOfRange(String, usize),
}

/// Parses a JSON text.
pub fn parse_json(text: &str) -> Result<JsonTree, JsonFailure> {
    let mut parser = JsonParser {
        chars: text.chars().collect(),
        position: 0,
    };
    let result = parser.value().and_then(|value| {
        parser.skip_whitespace();
        if parser.position == parser.chars.len() {
            Ok(value)
        } else {
            Err(parser.syntax("unexpected character after the JSON value"))
        }
    });
    result.map_err(|failure| {
        let (kind, text, position) = match failure {
            Failure::Syntax(message, position) => ("Syntax", message, position),
            Failure::NumberOutOfRange(number, position) => ("NumberOutOfRange", number, position),
        };
        let before = &parser.chars[..position.min(parser.chars.len())];
        let line = before.iter().filter(|c| **c == '\n').count() as i64 + 1;
        let column = before.iter().rev().take_while(|c| **c != '\n').count() as i64 + 1;
        JsonFailure { kind, text, line, column }
    })
}

pub(crate) struct JsonParser {
    chars: Vec<char>,
    position: usize,
}

impl JsonParser {
    fn syntax(&self, message: &str) -> Failure {
        Failure::Syntax(message.to_string(), self.position)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.chars.get(self.position), Some(c) if c.is_whitespace()) {
            self.position += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), Failure> {
        self.skip_whitespace();
        if self.chars.get(self.position) == Some(&c) {
            self.position += 1;
            Ok(())
        } else {
            Err(self.syntax(&format!("expected '{}'", c)))
        }
    }

    fn literal(&mut self, word: &str, value: JsonTree) -> Result<JsonTree, Failure> {
        for c in word.chars() {
            if self.chars.get(self.position) != Some(&c) {
                return Err(self.syntax("invalid literal"));
            }
            self.position += 1;
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<JsonTree, Failure> {
        self.skip_whitespace();
        match self.chars.get(self.position) {
            Some('{') => {
                self.position += 1;
                // A `Map[string, Json]`; a repeated key keeps its last value.
                let mut fields: Vec<(String, JsonTree)> = Vec::new();
                let object = JsonTree::Object;
                self.skip_whitespace();
                if self.chars.get(self.position) == Some(&'}') {
                    self.position += 1;
                    return Ok(object(fields));
                }
                loop {
                    self.skip_whitespace();
                    let key = self.string()?;
                    self.expect(':')?;
                    let value = self.value()?;
                    // A repeated key keeps its first position and its last value, like a map.
                    match fields.iter_mut().find(|(existing, _)| *existing == key) {
                        Some(entry) => entry.1 = value,
                        None => fields.push((key, value)),
                    }
                    self.skip_whitespace();
                    match self.chars.get(self.position) {
                        Some(',') => self.position += 1,
                        Some('}') => {
                            self.position += 1;
                            return Ok(object(fields));
                        }
                        _ => return Err(self.syntax("expected ',' or '}'")),
                    }
                }
            }
            Some('[') => {
                self.position += 1;
                let mut items = Vec::new();
                self.skip_whitespace();
                if self.chars.get(self.position) == Some(&']') {
                    self.position += 1;
                    return Ok(JsonTree::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.skip_whitespace();
                    match self.chars.get(self.position) {
                        Some(',') => self.position += 1,
                        Some(']') => {
                            self.position += 1;
                            return Ok(JsonTree::Array(items));
                        }
                        _ => return Err(self.syntax("expected ',' or ']'")),
                    }
                }
            }
            Some('"') => Ok(JsonTree::String(self.string()?)),
            Some('t') => self.literal("true", JsonTree::Bool(true)),
            Some('f') => self.literal("false", JsonTree::Bool(false)),
            Some('n') => self.literal("null", JsonTree::Null),
            Some(c) if *c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(self.syntax(&format!("unexpected character '{}'", c))),
            None => Err(self.syntax("unexpected end of input")),
        }
    }

    /// Numbers without `.`, `e` or `E` are `Int`; the others are `Float`.
    fn number(&mut self) -> Result<JsonTree, Failure> {
        let start = self.position;
        while matches!(
            self.chars.get(self.position),
            Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')
        ) {
            self.position += 1;
        }
        let text: String = self.chars[start..self.position].iter().collect();
        let out_of_range = || Failure::NumberOutOfRange(text.clone(), start);
        let invalid = || Failure::Syntax(format!("invalid number '{}'", text), start);
        if !text.contains(['.', 'e', 'E']) {
            let digits = text.strip_prefix('-').unwrap_or(&text);
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                return Err(invalid());
            }
            return match text.parse::<i64>() {
                Ok(n) => Ok(JsonTree::Int(n)),
                Err(_) => Err(out_of_range()),
            };
        }
        match text.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(JsonTree::Float(n)),
            Ok(_) => Err(out_of_range()),
            Err(_) => Err(invalid()),
        }
    }

    fn string(&mut self) -> Result<String, Failure> {
        if self.chars.get(self.position) != Some(&'"') {
            return Err(self.syntax("expected a string"));
        }
        self.position += 1;
        let mut out = String::new();
        loop {
            match self.chars.get(self.position).copied() {
                Some('"') => {
                    self.position += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.position += 1;
                    let escaped = self.chars.get(self.position).copied();
                    self.position += 1;
                    match escaped {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some('b') => out.push('\u{8}'),
                        Some('f') => out.push('\u{c}'),
                        Some('u') => {
                            let code = self.hex4()?;
                            let c = if (0xD800..0xDC00).contains(&code)
                                && self.chars.get(self.position) == Some(&'\\')
                                && self.chars.get(self.position + 1) == Some(&'u')
                            {
                                self.position += 2;
                                let low = self.hex4()?;
                                char::from_u32(
                                    0x10000 + ((code - 0xD800) << 10) + low.wrapping_sub(0xDC00),
                                )
                            } else {
                                char::from_u32(code)
                            };
                            out.push(c.unwrap_or('\u{FFFD}'));
                        }
                        Some(other) => out.push(other),
                        None => return Err(self.syntax("unterminated string")),
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.position += 1;
                }
                None => return Err(self.syntax("unterminated string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, Failure> {
        let text: String = self.chars.iter().skip(self.position).take(4).collect();
        let code = u32::from_str_radix(&text, 16)
            .map_err(|_| self.syntax(&format!("invalid escape '\\u{}'", text)))?;
        self.position += 4;
        Ok(code)
    }
}

/// A string in JSON, with its escapes.
pub fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A float in JSON: always with a decimal point or an exponent.
pub fn write_float(f: f64, out: &mut String) {
    let text = if f.abs() >= 1e16 {
        format!("{:e}", f)
    } else {
        format!("{}", f)
    };
    out.push_str(&text);
    if !text.contains(['.', 'e', 'E']) {
        out.push_str(".0");
    }
}

// The `json` module for compiled programs.

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

/// The descriptions a compiled program gives to build `Json` values.
struct Types {
    json: *const KojiRecordType,
    string: *const KojiElementType,
    item: *const KojiElementType,
}

fn build(tree: &JsonTree, types: &Types) -> *mut KojiRecord {
    match tree {
        JsonTree::Null => make(types.json, "Null", &[]),
        JsonTree::Bool(b) => make(types.json, "Bool", &[*b as u64]),
        JsonTree::Int(n) => make(types.json, "Int", &[*n as u64]),
        JsonTree::Float(f) => make(types.json, "Float", &[f.to_bits()]),
        JsonTree::String(s) => make(types.json, "String", &[slot(new_string(s.clone()))]),
        JsonTree::Array(items) => {
            let items = items.iter().map(|item| build(item, types) as usize as u64).collect();
            let list: *mut KojiList = new_list(types.item, items);
            make(types.json, "Array", &[list as usize as u64])
        }
        JsonTree::Object(fields) => {
            let mut map: *mut KojiMap = koji_map_new(types.string, types.item);
            for (key, value) in fields {
                let key = new_string(key.clone());
                let value = build(value, types);
                koji_map_set(&mut map, &key as *const _ as *const std::ffi::c_void, &value as *const _ as *const std::ffi::c_void);
                // The map keeps its own references.
                crate::string::koji_string_release(key);
                crate::record::koji_record_release(value);
            }
            make(types.json, "Object", &[map as usize as u64])
        }
    }
}

/// `json.parse(text)`: `Ok(Json)` or `Err(JsonError)`. `string` and `item` describe the keys
/// and the values of objects and arrays.
#[no_mangle]
pub extern "C" fn koji_std_json_parse(
    text: *const KojiString,
    result: *const KojiRecordType,
    string: *const KojiElementType,
    item: *const KojiElementType,
) -> *mut KojiRecord {
    match parse_json(string_text(text)) {
        Ok(tree) => {
            let types = Types { json: field_record(result, "Ok", 0), string, item };
            let value = build(&tree, &types);
            make(result, "Ok", &[value as usize as u64])
        }
        Err(failure) => {
            let error_type = field_record(result, "Err", 0);
            let error = make(
                error_type,
                failure.kind,
                &[slot(new_string(failure.text)), failure.line as u64, failure.column as u64],
            );
            make(result, "Err", &[error as usize as u64])
        }
    }
}

fn stringify(value: *const KojiRecord, out: &mut String) {
    let slots = record_slots(value);
    match record_variant(value).as_str() {
        "Null" => out.push_str("null"),
        "Bool" => {
            let _ = write!(out, "{}", slots[0] as u8 != 0);
        }
        "Int" => {
            let _ = write!(out, "{}", slots[0] as i64);
        }
        "Float" => write_float(f64::from_bits(slots[0]), out),
        "String" => write_string(string_text(slots[0] as usize as *const KojiString), out),
        "Array" => {
            let list = unsafe { &*(slots[0] as usize as *const KojiList) };
            out.push('[');
            for (index, item) in list.items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                stringify(*item as usize as *const KojiRecord, out);
            }
            out.push(']');
        }
        _ => {
            out.push('{');
            for (index, (key, value)) in map_entries(slots[0] as usize as *const KojiMap).iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(string_text(*key as usize as *const KojiString), out);
                out.push(':');
                stringify(*value as usize as *const KojiRecord, out);
            }
            out.push('}');
        }
    }
}

#[no_mangle]
pub extern "C" fn koji_std_json_stringify(value: *const KojiRecord) -> *mut KojiString {
    let mut out = String::new();
    stringify(value, &mut out);
    new_string(out)
}

/// `json.get(value, key)`: `Some` of a field of an object, or `None`.
#[no_mangle]
pub extern "C" fn koji_std_json_get(value: *const KojiRecord, key: *const KojiString, option: *const KojiRecordType) -> *mut KojiRecord {
    if record_variant(value) == "Object" {
        let map = record_slots(value)[0] as usize as *const KojiMap;
        let found = crate::map::koji_map_find(map, &key as *const _ as *const std::ffi::c_void);
        if !found.is_null() {
            let field = unsafe { *(found as *const u64) };
            crate::record::koji_record_retain(field as usize as *const KojiRecord);
            return make(option, "Some", &[field]);
        }
    }
    make(option, "None", &[])
}

/// `json.asString`, `asInt`, `asFloat` and `asBool`: `kind` is 0, 1, 2 or 3.
#[no_mangle]
pub extern "C" fn koji_std_json_as(value: *const KojiRecord, kind: i32, option: *const KojiRecordType) -> *mut KojiRecord {
    let slots = record_slots(value);
    let variant = record_variant(value);
    let found = match (kind, variant.as_str()) {
        (0, "String") => {
            crate::string::koji_string_retain(slots[0] as usize as *const KojiString);
            Some(slots[0])
        }
        (1, "Int") | (3, "Bool") => Some(slots[0]),
        (2, "Float") => Some(slots[0]),
        // An int is also a float.
        (2, "Int") => Some((slots[0] as i64 as f64).to_bits()),
        _ => None,
    };
    match found {
        Some(slot) => make(option, "Some", &[slot]),
        None => make(option, "None", &[]),
    }
}
