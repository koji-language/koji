//! Lists: a reference-counted array shared between copies (value semantics with copy-on-write).
//!
//! Every item takes one 8-byte slot (an int, a float, a boolean, or a pointer to a string or
//! another list), or two for a decimal: the items are 64-bit words, `stride` per item. The list does not know the type of its items: the generated code describes
//! it with a `KojiElementType`, whose functions retain, release, show, compare and order items.
//!
//! A function that changes a list takes the variable that holds it (`KojiList **`): if the list
//! is shared with another variable, it is copied first, so the other variable never sees the
//! change.

use std::cell::Cell;
use std::cmp::Ordering;
use std::ffi::{c_char, c_void};

use crate::output::panic_at;
use crate::string::{new_string, string_text, KojiString};

/// How the generated code describes the items of a list.
#[repr(C)]
pub struct KojiElementType {
    /// Bytes of an item (at most 16).
    pub size: usize,
    /// Adds a reference to the item in a slot (strings and lists), or nothing.
    pub retain: Option<unsafe extern "C" fn(*const c_void)>,
    /// Removes a reference to the item in a slot (strings and lists), or nothing.
    pub release: Option<unsafe extern "C" fn(*const c_void)>,
    /// The item as written in code (`'text'`, `[1, 2]`).
    pub repr: unsafe extern "C" fn(*const c_void) -> *mut KojiString,
    pub equal: unsafe extern "C" fn(*const c_void, *const c_void) -> bool,
    /// -1, 0 or 1; only for orderable items.
    pub compare: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> i32>,
    /// A hash that agrees with `equal` (for the keys of maps).
    pub hash: unsafe extern "C" fn(*const c_void) -> u64,
    /// The text `print` shows, when it is not the value as written in code (a string shows
    /// its text, not quotes), or `NULL`.
    pub display: Option<unsafe extern "C" fn(*const c_void) -> *mut KojiString>,
}

/// A list, laid out for C (the prelude reads its items and length, and counts its references,
/// inline): `items` points to `length * stride` words, in a buffer of `capacity` words that Rust
/// allocated as a `Vec<u64>`.
#[repr(C)]
pub struct KojiList {
    references: Cell<usize>,
    element: *const KojiElementType,
    stride: usize,
    items: *mut u64,
    length: usize,
    capacity: usize,
}

/// The words a value of this type takes: 1, or 2 for a decimal.
pub(crate) fn words(element: &KojiElementType) -> usize {
    if element.size > 8 {
        2
    } else {
        1
    }
}

/// A slot of two words, for a value given by the generated code (it fills `size` bytes).
pub(crate) type Slot = [u64; 2];

pub(crate) fn read_slot(element: &KojiElementType, value: *const c_void) -> Slot {
    let mut slot = [0u64; 2];
    unsafe { std::ptr::copy_nonoverlapping(value as *const u8, slot.as_mut_ptr() as *mut u8, element.size) };
    slot
}

impl KojiList {
    pub(crate) fn element(&self) -> &KojiElementType {
        unsafe { &*self.element }
    }

    /// The words of the items.
    pub(crate) fn words(&self) -> &[u64] {
        unsafe { std::slice::from_raw_parts(self.items, self.length * self.stride) }
    }

    fn words_mut(&mut self) -> &mut [u64] {
        unsafe { std::slice::from_raw_parts_mut(self.items, self.length * self.stride) }
    }

    /// The buffer of the items as a `Vec`, which the list no longer holds.
    fn take(&mut self) -> Vec<u64> {
        let items = unsafe { Vec::from_raw_parts(self.items, self.length * self.stride, self.capacity) };
        self.put(Vec::new());
        items
    }

    /// Holds `items` (their words) as the buffer of the list.
    fn put(&mut self, items: Vec<u64>) {
        let mut items = std::mem::ManuallyDrop::new(items);
        self.items = items.as_mut_ptr();
        self.length = items.len() / self.stride;
        self.capacity = items.capacity();
    }

    /// Changes the words of the items as a `Vec`.
    fn edit<R>(&mut self, change: impl FnOnce(&mut Vec<u64>) -> R) -> R {
        let mut items = self.take();
        let result = change(&mut items);
        self.put(items);
        result
    }

    pub(crate) fn slot(&self, index: usize) -> *const c_void {
        unsafe { self.items.add(index * self.stride) as *const c_void }
    }

    pub(crate) fn len(&self) -> usize {
        self.length
    }

    /// The words of the item at `index`.
    fn words_at(&self, index: usize) -> &[u64] {
        &self.words()[index * self.stride..(index + 1) * self.stride]
    }

    /// Puts the order of `order` (positions of items) in place.
    pub(crate) fn reorder(&mut self, order: &[usize]) {
        let mut items = Vec::with_capacity(self.words().len());
        for &index in order {
            items.extend_from_slice(self.words_at(index));
        }
        drop(self.take());
        self.put(items);
    }

    fn retain_item(&self, index: usize) {
        if let Some(retain) = self.element().retain {
            unsafe { retain(self.slot(index)) }
        }
    }

    fn release_item(&self, index: usize) {
        if let Some(release) = self.element().release {
            unsafe { release(self.slot(index)) }
        }
    }

    /// The words of an item given by the generated code.
    fn read(&self, value: *const c_void) -> Slot {
        read_slot(self.element(), value)
    }

    /// Copies the item at `index` into `out` (the generated code's variable).
    fn copy_out(&self, index: usize, out: *mut c_void) {
        unsafe { std::ptr::copy_nonoverlapping(self.slot(index) as *const u8, out as *mut u8, self.element().size) };
    }
}

/// A list of `items`: their words, `words(element)` for each one.
pub(crate) fn new_list(element: *const KojiElementType, items: Vec<u64>) -> *mut KojiList {
    let stride = words(unsafe { &*element });
    let mut list = KojiList { references: Cell::new(1), element, stride, items: std::ptr::NonNull::dangling().as_ptr(), length: 0, capacity: 0 };
    list.put(items);
    Box::into_raw(Box::new(list))
}

fn list<'a>(list: *const KojiList) -> &'a KojiList {
    unsafe { &*list }
}

/// A copy of `source`'s items, each with a new reference.
fn copy_items(source: &KojiList) -> Vec<u64> {
    let items = source.words().to_vec();
    for index in 0..source.len() {
        source.retain_item(index);
    }
    items
}

/// Makes the list in `variable` its own (copying it if it is shared) and returns it.
pub(crate) fn unshare<'a>(variable: *mut *mut KojiList) -> &'a mut KojiList {
    unsafe {
        let current = &**variable;
        if current.references.get() > 1 {
            let copy = new_list(current.element, copy_items(current));
            current.references.set(current.references.get() - 1);
            *variable = copy;
        }
        &mut **variable
    }
}

/// The position of `index` in a list of `length` items (`allow_end`: the position after the
/// last one, for `insert`), or a panic.
fn position(index: i64, length: usize, allow_end: bool, location: *const c_char) -> usize {
    if index >= 0 && ((index as usize) < length || (allow_end && index as usize == length)) {
        index as usize
    } else {
        panic_at(location, &format!("index {} out of range (length {})", index, length))
    }
}

#[no_mangle]
pub extern "C" fn koji_list_new(element: *const KojiElementType, capacity: usize) -> *mut KojiList {
    new_list(element, Vec::with_capacity(capacity * words(unsafe { &*element })))
}

#[no_mangle]
pub extern "C" fn koji_list_retain(list: *const KojiList) {
    if let Some(list) = unsafe { list.as_ref() } {
        list.references.set(list.references.get() + 1);
    }
}

/// Removes a reference; at zero, releases the items and frees the list. `NULL` is ignored.
#[no_mangle]
pub extern "C" fn koji_list_release(list: *mut KojiList) {
    let Some(reference) = (unsafe { list.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    koji_list_free(list);
}

/// Releases the items of a list without references and frees it.
#[no_mangle]
pub extern "C" fn koji_list_free(list: *mut KojiList) {
    let mut list = unsafe { Box::from_raw(list) };
    for index in 0..list.len() {
        list.release_item(index);
    }
    drop(list.take());
}

/// The panic of an index out of range (`koji_list_get` of the prelude).
#[no_mangle]
pub extern "C" fn koji_list_index_panic(index: i64, length: i64, location: *const c_char) -> ! {
    panic_at(location, &format!("index {} out of range (length {})", index, length))
}

#[no_mangle]
pub extern "C" fn koji_list_length(list: *const KojiList) -> i64 {
    self::list(list).len() as i64
}

/// The slot of the item at `index`, to read it.
#[no_mangle]
pub extern "C" fn koji_list_get(list: *const KojiList, index: i64, location: *const c_char) -> *const c_void {
    let list = self::list(list);
    list.slot(position(index, list.len(), false, location))
}

/// The slot of the item at `index`, to change it (`grid[0][1] = 5` changes the item of
/// `grid[0]` in place).
#[no_mangle]
pub extern "C" fn koji_list_slot(variable: *mut *mut KojiList, index: i64, location: *const c_char) -> *mut c_void {
    let list = unshare(variable);
    let index = position(index, list.len(), false, location);
    list.slot(index) as *mut c_void
}

/// `items[index] = value`.
#[no_mangle]
pub extern "C" fn koji_list_set(variable: *mut *mut KojiList, index: i64, value: *const c_void, location: *const c_char) {
    let list = unshare(variable);
    let index = position(index, list.len(), false, location);
    let slot = list.read(value);
    list.release_item(index);
    let stride = list.stride;
    list.words_mut()[index * stride..(index + 1) * stride].copy_from_slice(&slot[..stride]);
    list.retain_item(index);
}

/// `items.push(value)`.
#[no_mangle]
pub extern "C" fn koji_list_push(variable: *mut *mut KojiList, value: *const c_void) {
    let list = unshare(variable);
    let slot = list.read(value);
    let stride = list.stride;
    list.edit(|items| items.extend_from_slice(&slot[..stride]));
    list.retain_item(list.len() - 1);
}

/// `items.insert(index, value)`.
#[no_mangle]
pub extern "C" fn koji_list_insert(
    variable: *mut *mut KojiList,
    index: i64,
    value: *const c_void,
    location: *const c_char,
) {
    let list = unshare(variable);
    let index = position(index, list.len(), true, location);
    let slot = list.read(value);
    let stride = list.stride;
    list.edit(|items| {
        items.splice(index * stride..index * stride, slot[..stride].iter().copied());
    });
    list.retain_item(index);
}

/// `items.sort()`: a stable sort of orderable items.
#[no_mangle]
pub extern "C" fn koji_list_sort(variable: *mut *mut KojiList) {
    let list = unshare(variable);
    let Some(compare) = list.element().compare else {
        return;
    };
    let mut order: Vec<usize> = (0..list.len()).collect();
    order.sort_by(|&a, &b| match unsafe { compare(list.slot(a), list.slot(b)) } {
        n if n < 0 => Ordering::Less,
        0 => Ordering::Equal,
        _ => Ordering::Greater,
    });
    list.reorder(&order);
}

/// `a + b`: a new list.
#[no_mangle]
pub extern "C" fn koji_list_concat(a: *const KojiList, b: *const KojiList) -> *mut KojiList {
    let (a, b) = (list(a), list(b));
    let mut items = copy_items(a);
    items.extend(copy_items(b));
    new_list(a.element, items)
}

/// `items[start..end]`, where a missing bound is the start or the end of the list.
#[no_mangle]
pub extern "C" fn koji_list_slice(
    list: *const KojiList,
    has_start: bool,
    start: i64,
    has_end: bool,
    end: i64,
    location: *const c_char,
) -> *mut KojiList {
    let list = self::list(list);
    let length = list.len() as i64;
    let (start, end) = (if has_start { start } else { 0 }, if has_end { end } else { length });
    if start < 0 || end > length || start > end {
        panic_at(location, &format!("slice {}..{} out of range (length {})", start, end, length));
    }
    let items = list.words()[start as usize * list.stride..end as usize * list.stride].to_vec();
    for index in start as usize..end as usize {
        list.retain_item(index);
    }
    new_list(list.element, items)
}

#[no_mangle]
pub extern "C" fn koji_list_equal(a: *const KojiList, b: *const KojiList) -> bool {
    let (a, b) = (list(a), list(b));
    std::ptr::eq(a, b)
        || (a.len() == b.len()
            && (0..a.len()).all(|index| unsafe { (a.element().equal)(a.slot(index), b.slot(index)) }))
}

/// `value in items`.
#[no_mangle]
pub extern "C" fn koji_list_contains(list: *const KojiList, value: *const c_void) -> bool {
    let list = self::list(list);
    let slot = list.read(value);
    let value = slot.as_ptr() as *const c_void;
    (0..list.len()).any(|index| unsafe { (list.element().equal)(list.slot(index), value) })
}

/// The list as written in code: `[1, 2, 3]`, `['a', 'b']`.
#[no_mangle]
pub extern "C" fn koji_list_repr(list: *const KojiList) -> *mut KojiString {
    let list = self::list(list);
    let mut text = String::from("[");
    for index in 0..list.len() {
        if index > 0 {
            text.push_str(", ");
        }
        let item = unsafe { (list.element().repr)(list.slot(index)) };
        text.push_str(string_text(item));
        crate::string::koji_string_release(item);
    }
    text.push(']');
    new_string(text)
}

/// `items.join(separator)`, for a list of strings.
#[no_mangle]
pub extern "C" fn koji_list_join(list: *const KojiList, separator: *const KojiString) -> *mut KojiString {
    let list = self::list(list);
    let separator = string_text(separator);
    let mut text = String::new();
    for (index, slot) in list.words().iter().enumerate() {
        if index > 0 {
            text.push_str(separator);
        }
        text.push_str(string_text(*slot as usize as *const KojiString));
    }
    new_string(text)
}

/// `items.pop()`: copies the last item into `out` (the caller now owns its reference) and
/// returns true, or returns false for an empty list.
#[no_mangle]
pub extern "C" fn koji_list_pop(variable: *mut *mut KojiList, out: *mut c_void) -> bool {
    let list = unshare(variable);
    if list.len() == 0 {
        return false;
    }
    let last = list.len() - 1;
    list.copy_out(last, out);
    let stride = list.stride;
    list.edit(|items| items.truncate(last * stride));
    true
}

/// `items.removeAt(index)`: like `koji_list_pop`, for the item at `index`; false when `index`
/// is out of range.
#[no_mangle]
pub extern "C" fn koji_list_remove(variable: *mut *mut KojiList, index: i64, out: *mut c_void) -> bool {
    let list = unshare(variable);
    if index < 0 || index as usize >= list.len() {
        return false;
    }
    let index = index as usize;
    list.copy_out(index, out);
    let stride = list.stride;
    list.edit(|items| {
        items.drain(index * stride..(index + 1) * stride);
    });
    true
}

/// `items.indexOf(value)`: the position of the first equal item, or -1.
#[no_mangle]
pub extern "C" fn koji_list_index_of(list: *const KojiList, value: *const c_void) -> i64 {
    let list = self::list(list);
    let slot = list.read(value);
    let value = slot.as_ptr() as *const c_void;
    (0..list.len())
        .position(|index| unsafe { (list.element().equal)(list.slot(index), value) })
        .map_or(-1, |index| index as i64)
}

/// A hash of the items, in order.
#[no_mangle]
pub extern "C" fn koji_list_hash(list: *const KojiList) -> u64 {
    use std::hash::{Hash, Hasher};
    let list = self::list(list);
    let mut hasher = crate::hash::Fast::default();
    for index in 0..list.len() {
        unsafe { (list.element().hash)(list.slot(index)) }.hash(&mut hasher);
    }
    hasher.finish()
}
