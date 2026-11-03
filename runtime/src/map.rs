//! Maps and sets: entries kept in the order their keys were added, with a hash index to find
//! a key. A set is a map without values. Like lists, they are reference-counted and copied
//! when a shared one is changed.

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};

use crate::list::{new_list, KojiElementType, KojiList};
use crate::output::panic_at;
use crate::string::{new_string, string_text, KojiString};

pub struct KojiMap {
    references: Cell<usize>,
    key: *const KojiElementType,
    /// `NULL` for a set.
    value: *const KojiElementType,
    entries: Vec<(u64, u64)>,
    index: HashMap<u64, Vec<usize>>,
}

fn slot_of(slot: &u64) -> *const c_void {
    slot as *const u64 as *const c_void
}

/// The slot for a value given by the generated code.
fn read(element: &KojiElementType, value: *const c_void) -> u64 {
    let mut slot = 0u64;
    unsafe { std::ptr::copy_nonoverlapping(value as *const u8, &mut slot as *mut u64 as *mut u8, element.size) };
    slot
}

impl KojiMap {
    fn key_type(&self) -> &KojiElementType {
        unsafe { &*self.key }
    }

    fn value_type(&self) -> Option<&KojiElementType> {
        unsafe { self.value.as_ref() }
    }

    fn hash(&self, key: *const c_void) -> u64 {
        unsafe { (self.key_type().hash)(key) }
    }

    fn position(&self, key: *const c_void) -> Option<usize> {
        let positions = self.index.get(&self.hash(key))?;
        positions
            .iter()
            .copied()
            .find(|&i| unsafe { (self.key_type().equal)(slot_of(&self.entries[i].0), key) })
    }

    fn retain_entry(&self, index: usize) {
        let (key, value) = &self.entries[index];
        if let Some(retain) = self.key_type().retain {
            unsafe { retain(slot_of(key)) }
        }
        if let Some(retain) = self.value_type().and_then(|t| t.retain) {
            unsafe { retain(slot_of(value)) }
        }
    }

    fn release_entry(&self, index: usize) {
        let (key, value) = &self.entries[index];
        if let Some(release) = self.key_type().release {
            unsafe { release(slot_of(key)) }
        }
        if let Some(release) = self.value_type().and_then(|t| t.release) {
            unsafe { release(slot_of(value)) }
        }
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for index in 0..self.entries.len() {
            let hash = self.hash(slot_of(&self.entries[index].0));
            self.index.entry(hash).or_default().push(index);
        }
    }

    fn missing(&self, key: *const c_void, location: *const c_char) -> ! {
        let text = unsafe { (self.key_type().repr)(key) };
        panic_at(location, &format!("key {} not found", string_text(text)))
    }
}

fn map<'a>(map: *const KojiMap) -> &'a KojiMap {
    unsafe { &*map }
}

fn new_map(key: *const KojiElementType, value: *const KojiElementType) -> KojiMap {
    KojiMap {
        references: Cell::new(1),
        key,
        value,
        entries: Vec::new(),
        index: HashMap::new(),
    }
}

/// Makes the map in `variable` its own (copying it if it is shared) and returns it.
fn unshare<'a>(variable: *mut *mut KojiMap) -> &'a mut KojiMap {
    unsafe {
        let current = &**variable;
        if current.references.get() > 1 {
            let mut copy = new_map(current.key, current.value);
            copy.entries = current.entries.clone();
            copy.index = current.index.clone();
            for index in 0..copy.entries.len() {
                copy.retain_entry(index);
            }
            current.references.set(current.references.get() - 1);
            *variable = Box::into_raw(Box::new(copy));
        }
        &mut **variable
    }
}

/// A new empty map (`value` is `NULL` for a set).
#[no_mangle]
pub extern "C" fn koji_map_new(key: *const KojiElementType, value: *const KojiElementType) -> *mut KojiMap {
    Box::into_raw(Box::new(new_map(key, value)))
}

#[no_mangle]
pub extern "C" fn koji_map_retain(map: *const KojiMap) {
    if let Some(map) = unsafe { map.as_ref() } {
        map.references.set(map.references.get() + 1);
    }
}

#[no_mangle]
pub extern "C" fn koji_map_release(map: *mut KojiMap) {
    let Some(reference) = (unsafe { map.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    for index in 0..reference.entries.len() {
        reference.release_entry(index);
    }
    drop(unsafe { Box::from_raw(map) });
}

#[no_mangle]
pub extern "C" fn koji_map_length(map: *const KojiMap) -> i64 {
    self::map(map).entries.len() as i64
}

/// `map[key]`: the slot of the value (panics when the key is missing).
#[no_mangle]
pub extern "C" fn koji_map_get(map: *const KojiMap, key: *const c_void, location: *const c_char) -> *const c_void {
    let map = self::map(map);
    match map.position(key) {
        Some(index) => slot_of(&map.entries[index].1),
        None => map.missing(key, location),
    }
}

/// The slot of the value of `key`, or `NULL` (`map.get(key)`).
#[no_mangle]
pub extern "C" fn koji_map_find(map: *const KojiMap, key: *const c_void) -> *const c_void {
    let map = self::map(map);
    match map.position(key) {
        Some(index) => slot_of(&map.entries[index].1),
        None => std::ptr::null(),
    }
}

/// `key in map`, `set.has(item)`.
#[no_mangle]
pub extern "C" fn koji_map_contains(map: *const KojiMap, key: *const c_void) -> bool {
    self::map(map).position(key).is_some()
}

/// The slot of the value of `key`, to change it in place (`groups['a'].push(1)`).
#[no_mangle]
pub extern "C" fn koji_map_slot(variable: *mut *mut KojiMap, key: *const c_void, location: *const c_char) -> *mut c_void {
    let map = unshare(variable);
    match map.position(key) {
        Some(index) => &mut map.entries[index].1 as *mut u64 as *mut c_void,
        None => map.missing(key, location),
    }
}

/// `map[key] = value` (adds or replaces), or `set.add(item)` with a `NULL` value.
#[no_mangle]
pub extern "C" fn koji_map_set(variable: *mut *mut KojiMap, key: *const c_void, value: *const c_void) {
    let map = unshare(variable);
    let value_slot = match map.value_type() {
        Some(value_type) => read(value_type, value),
        None => 0,
    };
    match map.position(key) {
        Some(index) => {
            // Not borrowed from `map`: the description lives as long as the program.
            if let Some(value_type) = unsafe { map.value.as_ref() } {
                if let Some(release) = value_type.release {
                    unsafe { release(slot_of(&map.entries[index].1)) }
                }
                map.entries[index].1 = value_slot;
                if let Some(retain) = value_type.retain {
                    unsafe { retain(slot_of(&map.entries[index].1)) }
                }
            }
        }
        None => {
            let key_slot = read(map.key_type(), key);
            let hash = map.hash(key);
            map.entries.push((key_slot, value_slot));
            let index = map.entries.len() - 1;
            map.index.entry(hash).or_default().push(index);
            map.retain_entry(index);
        }
    }
}

/// `map.remove(key)`: copies the value into `out` (the caller now owns it) and returns true,
/// or returns false when the key is missing. For a set, `out` is ignored.
#[no_mangle]
pub extern "C" fn koji_map_remove(variable: *mut *mut KojiMap, key: *const c_void, out: *mut c_void) -> bool {
    let map = unshare(variable);
    let Some(index) = map.position(key) else {
        return false;
    };
    let (key_slot, value_slot) = map.entries.remove(index);
    if let Some(release) = map.key_type().release {
        unsafe { release(slot_of(&key_slot)) }
    }
    if let Some(value_type) = map.value_type() {
        unsafe { std::ptr::copy_nonoverlapping(&value_slot as *const u64 as *const u8, out as *mut u8, value_type.size) };
    }
    map.rebuild_index();
    true
}

/// The keys (`map.keys()`, the items of a set), in the order they were added.
#[no_mangle]
pub extern "C" fn koji_map_keys(map: *const KojiMap) -> *mut KojiList {
    let map = self::map(map);
    let list = new_list(map.key, map.entries.iter().map(|(key, _)| *key).collect());
    if let Some(retain) = map.key_type().retain {
        for (key, _) in &map.entries {
            unsafe { retain(slot_of(key)) }
        }
    }
    list
}

/// The values (`map.values()`), in the order their keys were added.
#[no_mangle]
pub extern "C" fn koji_map_values(map: *const KojiMap) -> *mut KojiList {
    let map = self::map(map);
    let list = new_list(map.value, map.entries.iter().map(|(_, value)| *value).collect());
    if let Some(retain) = map.value_type().and_then(|t| t.retain) {
        for (_, value) in &map.entries {
            unsafe { retain(slot_of(value)) }
        }
    }
    list
}

/// `Set.from(list)`.
#[no_mangle]
pub extern "C" fn koji_set_from_list(list: *const KojiList) -> *mut KojiMap {
    let list = unsafe { &*list };
    let mut set = Box::into_raw(Box::new(new_map(list.element() as *const KojiElementType, std::ptr::null())));
    for index in 0..list.items.len() {
        koji_map_set(&mut set, list.slot(index), std::ptr::null());
    }
    set
}

/// Maps with the same entries, in any order.
#[no_mangle]
pub extern "C" fn koji_map_equal(a: *const KojiMap, b: *const KojiMap) -> bool {
    let (a, b) = (map(a), map(b));
    if std::ptr::eq(a, b) {
        return true;
    }
    a.entries.len() == b.entries.len()
        && a.entries.iter().all(|(key, value)| match b.position(slot_of(key)) {
            None => false,
            Some(index) => match a.value_type() {
                None => true,
                Some(value_type) => unsafe { (value_type.equal)(slot_of(value), slot_of(&b.entries[index].1)) },
            },
        })
}

/// A hash that does not depend on the order of the entries.
#[no_mangle]
pub extern "C" fn koji_map_hash(map: *const KojiMap) -> u64 {
    let map = self::map(map);
    map.entries.iter().fold(0u64, |total, (key, value)| {
        let value_hash = match map.value_type() {
            Some(value_type) => unsafe { (value_type.hash)(slot_of(value)) },
            None => 0,
        };
        total.wrapping_add(map.hash(slot_of(key)).wrapping_mul(31).wrapping_add(value_hash))
    })
}

/// `['Ana': 30]`, `[:]` or `Set(['a', 'b'])`.
#[no_mangle]
pub extern "C" fn koji_map_repr(map: *const KojiMap) -> *mut KojiString {
    let map = self::map(map);
    let repr = |element: &KojiElementType, slot: &u64| -> String {
        let text = unsafe { (element.repr)(slot_of(slot)) };
        let result = string_text(text).to_string();
        crate::string::koji_string_release(text);
        result
    };
    let mut text = String::new();
    match map.value_type() {
        None => {
            text.push_str("Set([");
            for (index, (key, _)) in map.entries.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&repr(map.key_type(), key));
            }
            text.push_str("])");
        }
        Some(_) if map.entries.is_empty() => text.push_str("[:]"),
        Some(value_type) => {
            text.push('[');
            for (index, (key, value)) in map.entries.iter().enumerate() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&repr(map.key_type(), key));
                text.push_str(": ");
                text.push_str(&repr(value_type, value));
            }
            text.push(']');
        }
    }
    new_string(text)
}

/// The entries (key and value slots), in the order their keys were added.
pub(crate) fn map_entries(map: *const KojiMap) -> Vec<(u64, u64)> {
    self::map(map).entries.clone()
}
