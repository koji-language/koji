//! Maps and sets: entries kept in the order their keys were added, with a hash index to find
//! a key: a table of positions in the entries, open addressing, at most half full. Each entry
//! keeps the hash of its key, so the table is built again without hashing the keys. A set is a map without values. Like lists, they are reference-counted and copied
//! when a shared one is changed.

use std::cell::Cell;
use std::ffi::{c_char, c_void};

use crate::list::{new_list, read_slot, words, KojiElementType, KojiList, Slot};
use crate::output::panic_at;
use crate::string::{new_string, string_text, KojiString};

pub struct KojiMap {
    references: Cell<usize>,
    key: *const KojiElementType,
    /// `NULL` for a set.
    value: *const KojiElementType,
    /// Keys and values, in two words each (a decimal takes both).
    entries: Vec<(Slot, Slot)>,
    /// The hash of the key of each entry.
    hashes: Vec<u64>,
    /// For each slot, a position in `entries` plus one, or 0 for none (a power of two of slots,
    /// or none for an empty map).
    table: Vec<u32>,
}

/// A large odd number: the high bits of a hash times it depend on all the bits of the hash (an
/// int is its own hash).
const SPREAD: u64 = 0x9e37_79b9_7f4a_7c15;

fn slot_of(slot: &Slot) -> *const c_void {
    slot.as_ptr() as *const c_void
}

/// The slot for a value given by the generated code.
fn read(element: &KojiElementType, value: *const c_void) -> Slot {
    read_slot(element, value)
}

/// The words of slots for a list of `element`.
fn list_words<'a>(element: &KojiElementType, slots: impl Iterator<Item = &'a Slot>) -> Vec<u64> {
    let count = words(element);
    slots.flat_map(|slot| slot[..count].iter().copied()).collect()
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
        self.find(key, self.hash(key))
    }

    /// The first slot of the table to look at for `hash`.
    fn start(&self, hash: u64) -> usize {
        let bits = self.table.len().trailing_zeros();
        (hash.wrapping_mul(SPREAD) >> (64 - bits)) as usize
    }

    fn find(&self, key: *const c_void, hash: u64) -> Option<usize> {
        if self.table.is_empty() {
            return None;
        }
        let mask = self.table.len() - 1;
        let mut slot = self.start(hash);
        loop {
            let position = self.table[slot] as usize;
            if position == 0 {
                return None;
            }
            let index = position - 1;
            if self.hashes[index] == hash && unsafe { (self.key_type().equal)(slot_of(&self.entries[index].0), key) } {
                return Some(index);
            }
            slot = (slot + 1) & mask;
        }
    }

    /// Puts the entry `index` in the table, which has room for it.
    fn place(&mut self, index: usize) {
        let mask = self.table.len() - 1;
        let mut slot = self.start(self.hashes[index]);
        while self.table[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        self.table[slot] = index as u32 + 1;
    }

    /// Adds an entry (its key is not in the map).
    fn add(&mut self, key: Slot, value: Slot, hash: u64) {
        assert!(self.entries.len() < u32::MAX as usize, "too many entries in a map");
        self.entries.push((key, value));
        self.hashes.push(hash);
        if self.entries.len() * 2 > self.table.len() {
            self.rebuild_index();
        } else {
            self.place(self.entries.len() - 1);
        }
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

    /// The table again, at least twice as large as the entries.
    fn rebuild_index(&mut self) {
        let mut size = 8;
        while size < self.entries.len() * 2 {
            size *= 2;
        }
        self.table.clear();
        self.table.resize(size, 0);
        for index in 0..self.entries.len() {
            self.place(index);
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
        hashes: Vec::new(),
        table: Vec::new(),
    }
}

/// The words of a `KojiMap`.
const MAP_WORDS: usize = std::mem::size_of::<KojiMap>() / 8;

/// A map in a block of the pool, like records and lists: maps are many and short-lived.
fn boxed(map: KojiMap) -> *mut KojiMap {
    let block = crate::pool::take(MAP_WORDS) as *mut KojiMap;
    unsafe { block.write(map) };
    block
}

/// Makes the map in `variable` its own (copying it if it is shared) and returns it.
fn unshare<'a>(variable: *mut *mut KojiMap) -> &'a mut KojiMap {
    unsafe {
        let current = &**variable;
        if current.references.get() > 1 {
            let mut copy = new_map(current.key, current.value);
            copy.entries = current.entries.clone();
            copy.hashes = current.hashes.clone();
            copy.table = current.table.clone();
            for index in 0..copy.entries.len() {
                copy.retain_entry(index);
            }
            current.references.set(current.references.get() - 1);
            *variable = boxed(copy);
        }
        &mut **variable
    }
}

/// A new empty map (`value` is `NULL` for a set).
#[no_mangle]
pub extern "C" fn koji_map_new(key: *const KojiElementType, value: *const KojiElementType) -> *mut KojiMap {
    boxed(new_map(key, value))
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
    unsafe { std::ptr::drop_in_place(map) };
    crate::pool::give(map as *mut u8, MAP_WORDS);
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
        Some(index) => map.entries[index].1.as_mut_ptr() as *mut c_void,
        None => map.missing(key, location),
    }
}

/// `map[key] = value` (adds or replaces), or `set.add(item)` with a `NULL` value.
#[no_mangle]
pub extern "C" fn koji_map_set(variable: *mut *mut KojiMap, key: *const c_void, value: *const c_void) {
    let map = unshare(variable);
    let value_slot = match map.value_type() {
        Some(value_type) => read(value_type, value),
        None => [0, 0],
    };
    let hash = map.hash(key);
    match map.find(key, hash) {
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
            map.add(key_slot, value_slot, hash);
            map.retain_entry(map.entries.len() - 1);
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
    map.hashes.remove(index);
    if let Some(release) = map.key_type().release {
        unsafe { release(slot_of(&key_slot)) }
    }
    if let Some(value_type) = map.value_type() {
        unsafe { std::ptr::copy_nonoverlapping(value_slot.as_ptr() as *const u8, out as *mut u8, value_type.size) };
    }
    map.rebuild_index();
    true
}

/// The keys (`map.keys()`, the items of a set), in the order they were added.
#[no_mangle]
pub extern "C" fn koji_map_keys(map: *const KojiMap) -> *mut KojiList {
    let map = self::map(map);
    let list = new_list(map.key, list_words(map.key_type(), map.entries.iter().map(|(key, _)| key)));
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
    let value_type = map.value_type().expect("the values of a map");
    let list = new_list(map.value, list_words(value_type, map.entries.iter().map(|(_, value)| value)));
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
    let mut set = boxed(new_map(list.element() as *const KojiElementType, std::ptr::null()));
    for index in 0..list.len() {
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
    let repr = |element: &KojiElementType, slot: &Slot| -> String {
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
