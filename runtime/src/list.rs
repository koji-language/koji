//! The layout of lists, for the runtime in Rust that still makes and sorts them (maps, `sortBy`):
//! lists are in std/koji/list.kj. A reference-counted array shared between copies (value
//! semantics with copy-on-write).
//!
//! Every item takes one 8-byte slot (an int, a float, a boolean, or a pointer to a string or
//! another list), or two for a decimal: the items are 64-bit words, `stride` per item. The list does not know the type of its items: the generated code describes
//! it with a `KojiElementType`, whose functions retain, release, show, compare and order items.
//!
//! A function that changes a list takes the variable that holds it (`KojiList **`): if the list
//! is shared with another variable, it is copied first, so the other variable never sees the
//! change.

use std::cell::Cell;
use std::ffi::c_void;

use crate::string::KojiString;

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

}

/// A list of `items`: their words, `words(element)` for each one.
pub(crate) fn new_list(element: *const KojiElementType, items: Vec<u64>) -> *mut KojiList {
    let stride = words(unsafe { &*element });
    let mut list = KojiList { references: Cell::new(1), element, stride, items: std::ptr::NonNull::dangling().as_ptr(), length: 0, capacity: 0 };
    list.put(items);
    // In the pool, like records: lists are many and short-lived.
    let block = crate::pool::take(LIST_WORDS) as *mut KojiList;
    unsafe { block.write(list) };
    block
}

/// The words of a `KojiList`.
const LIST_WORDS: usize = std::mem::size_of::<KojiList>() / 8;

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
