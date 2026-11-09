//! Records: the values of declared types (objects and the variants of sum types, including
//! `Option` and `Result`). Like lists, they are reference-counted and copied when a shared one
//! is changed. Each field takes one 8-byte word, or two for a decimal, described by a
//! `KojiElementType`.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};

use crate::list::KojiElementType;
use crate::string::{new_string, string_text, KojiString};

/// One variant of a sum type, or the fields of an object type.
#[repr(C)]
pub struct KojiVariantType {
    /// The variant's name (`NULL` for an object type).
    pub name: *const c_char,
    pub field_count: usize,
    pub field_names: *const *const c_char,
    pub field_types: *const *const KojiElementType,
    /// For each field of a declared type, its description (`NULL` for the others), so the
    /// runtime can build the values of the standard library (`Err(InvalidNumber('x'))`).
    pub field_records: *const *const KojiRecordType,
    /// Where each field starts, in words, then the number of words (`NULL` when every field
    /// takes one word: field `i` is at word `i`).
    pub field_offsets: *const usize,
}

impl KojiVariantType {
    fn offset(&self, index: usize) -> usize {
        if self.field_offsets.is_null() {
            index
        } else {
            unsafe { *self.field_offsets.add(index) }
        }
    }

    fn word_count(&self) -> usize {
        self.offset(self.field_count)
    }
}

/// A declared type, as described by the generated code.
#[repr(C)]
pub struct KojiRecordType {
    /// The name of the type, without type arguments (`Result`, `MathError`).
    pub name: *const c_char,
    pub variant_count: usize,
    pub variants: *const KojiVariantType,
    /// Releases the fields of a record of the type and gives its memory back, generated for the
    /// type (the prelude calls it inline). Not read here: `koji_record_free` does the same for any
    /// type.
    pub free: Option<unsafe extern "C" fn(*mut KojiRecord)>,
    /// For an opaque type, the text of a value (its `display` method): the type hides its fields,
    /// so a value is shown so inside other values and by `debug()` too. `NULL` for other types.
    pub display: Option<unsafe extern "C" fn(*const KojiRecord) -> *mut KojiString>,
}

/// A record, laid out for C (the prelude reads its fields and counts its references inline): the
/// words of its fields follow it, in the same allocation (see `KojiVariantType::field_offsets`).
#[repr(C)]
pub struct KojiRecord {
    references: Cell<usize>,
    ty: *const KojiRecordType,
    tag: usize,
    fields: [u64; 0],
}


/// The 8-byte words of a record with `fields` words of fields.
fn block_words(fields: usize) -> usize {
    std::mem::size_of::<KojiRecord>() / 8 + fields
}

/// A new record of variant `tag`, with one reference and its fields zero.
#[inline]
fn allocate(ty: *const KojiRecordType, tag: usize) -> *mut KojiRecord {
    let words = unsafe { (*(*ty).variants.add(tag)).word_count() };
    unsafe {
        let record = crate::pool::take(block_words(words)) as *mut KojiRecord;
        record.write(KojiRecord { references: Cell::new(1), ty, tag, fields: [] });
        std::ptr::write_bytes((*record).fields.as_mut_ptr(), 0, words);
        record
    }
}

impl KojiRecord {
    /// The words of the fields.
    fn words(&self) -> &[u64] {
        unsafe { std::slice::from_raw_parts(self.fields.as_ptr(), self.variant().word_count()) }
    }

    fn words_mut(&mut self) -> &mut [u64] {
        let count = self.variant().word_count();
        unsafe { std::slice::from_raw_parts_mut(self.fields.as_mut_ptr(), count) }
    }

    fn variant(&self) -> &KojiVariantType {
        unsafe { &*(*self.ty).variants.add(self.tag) }
    }

    fn field_type(&self, index: usize) -> &KojiElementType {
        unsafe { &**self.variant().field_types.add(index) }
    }

    fn field_name(&self, index: usize) -> String {
        unsafe { CStr::from_ptr(*self.variant().field_names.add(index)) }
            .to_string_lossy()
            .into_owned()
    }

    fn slot(&self, index: usize) -> *const c_void {
        unsafe { self.fields.as_ptr().add(self.variant().offset(index)) as *const c_void }
    }

    fn field_count(&self) -> usize {
        self.variant().field_count
    }

    /// Copies a value given by the generated code into field `index`.
    fn store(&mut self, index: usize, value: *const c_void) {
        let size = self.field_type(index).size;
        let offset = self.variant().offset(index);
        unsafe { std::ptr::copy_nonoverlapping(value as *const u8, self.fields.as_mut_ptr().add(offset) as *mut u8, size) };
    }

    fn retain_field(&self, index: usize) {
        if let Some(retain) = self.field_type(index).retain {
            unsafe { retain(self.slot(index)) }
        }
    }

    fn release_field(&self, index: usize) {
        if let Some(release) = self.field_type(index).release {
            unsafe { release(self.slot(index)) }
        }
    }
}

fn record<'a>(record: *const KojiRecord) -> &'a KojiRecord {
    unsafe { &*record }
}

/// A new value of variant `tag` of `ty`, with its fields still empty: the generated code sets
/// each one with `koji_record_init`.
#[no_mangle]
pub extern "C" fn koji_record_new(ty: *const KojiRecordType, tag: usize) -> *mut KojiRecord {
    allocate(ty, tag)
}

/// Sets a field of a new record (keeping its own reference to the value).
#[no_mangle]
pub extern "C" fn koji_record_init(record: *mut KojiRecord, index: usize, value: *const c_void) {
    let record = unsafe { &mut *record };
    record.store(index, value);
    record.retain_field(index);
}

#[no_mangle]
pub extern "C" fn koji_record_retain(record: *const KojiRecord) {
    if let Some(record) = unsafe { record.as_ref() } {
        record.references.set(record.references.get() + 1);
    }
}

/// Removes a reference; at zero, releases the fields and frees the record. `NULL` is ignored.
/// (The prelude does the same inline, and calls `koji_record_free` at zero.)
#[no_mangle]
pub extern "C" fn koji_record_release(record: *mut KojiRecord) {
    let Some(reference) = (unsafe { record.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    koji_record_free(record);
}

/// Releases the fields of a record without references and frees it.
#[no_mangle]
pub extern "C" fn koji_record_free(record: *mut KojiRecord) {
    let reference = unsafe { &*record };
    for index in 0..reference.field_count() {
        reference.release_field(index);
    }
    let words = reference.variant().word_count();
    crate::pool::give(record as *mut u8, block_words(words));
}

/// The variant of a value of a sum type (its position in the declaration).
#[no_mangle]
pub extern "C" fn koji_record_tag(record: *const KojiRecord) -> i64 {
    self::record(record).tag as i64
}

/// The slot of a field, to read it.
#[no_mangle]
pub extern "C" fn koji_record_get(record: *const KojiRecord, index: usize) -> *const c_void {
    self::record(record).slot(index)
}

/// Makes the record in `variable` its own, for the generated code that changes a field in place
/// (it calls this only when the record is shared).
#[no_mangle]
pub extern "C" fn koji_record_unshare(variable: *mut *mut KojiRecord) {
    unshare(variable);
}

/// Makes the record in `variable` its own (copying it if it is shared) and returns it.
fn unshare<'a>(variable: *mut *mut KojiRecord) -> &'a mut KojiRecord {
    unsafe {
        let current = &**variable;
        if current.references.get() > 1 {
            let copy = allocate(current.ty, current.tag);
            (*copy).words_mut().copy_from_slice(current.words());
            for index in 0..(*copy).field_count() {
                (*copy).retain_field(index);
            }
            current.references.set(current.references.get() - 1);
            *variable = copy;
        }
        &mut **variable
    }
}

/// The slot of a field, to change what it holds in place (`people[0].tags.push('a')`).
#[no_mangle]
pub extern "C" fn koji_record_slot(variable: *mut *mut KojiRecord, index: usize) -> *mut c_void {
    let record = unshare(variable);
    record.slot(index) as *mut c_void
}

/// `record.field = value`.
#[no_mangle]
pub extern "C" fn koji_record_set(variable: *mut *mut KojiRecord, index: usize, value: *const c_void) {
    let record = unshare(variable);
    // Read first: `value` may be inside the old value of the field.
    let slot = crate::list::read_slot(record.field_type(index), value);
    record.release_field(index);
    record.store(index, slot.as_ptr() as *const c_void);
    record.retain_field(index);
}

#[no_mangle]
pub extern "C" fn koji_record_equal(a: *const KojiRecord, b: *const KojiRecord) -> bool {
    let (a, b) = (record(a), record(b));
    std::ptr::eq(a, b)
        || (a.tag == b.tag
            && (0..a.field_count()).all(|index| unsafe { (a.field_type(index).equal)(a.slot(index), b.slot(index)) }))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// The value as written in code: `{ x: 1, y: 2 }`, `Some(1)`, `None`.
#[no_mangle]
pub extern "C" fn koji_record_repr(record: *const KojiRecord) -> *mut KojiString {
    let record = self::record(record);
    if let Some(display) = unsafe { (*record.ty).display } {
        return unsafe { display(record) };
    }
    let field = |index: usize| -> String {
        let item = unsafe { (record.field_type(index).repr)(record.slot(index)) };
        let text = string_text(item).to_string();
        crate::string::koji_string_release(item);
        text
    };
    let mut text = String::new();
    let name = record.variant().name;
    if name.is_null() {
        if record.field_count() == 0 {
            return new_string("{}".to_string());
        }
        text.push_str("{ ");
        for index in 0..record.field_count() {
            if index > 0 {
                text.push_str(", ");
            }
            let name = record.field_name(index);
            if is_identifier(&name) {
                text.push_str(&name);
            } else {
                text.push_str(&crate::string::repr_string(&name));
            }
            text.push_str(": ");
            text.push_str(&field(index));
        }
        text.push_str(" }");
    } else {
        text.push_str(&unsafe { CStr::from_ptr(name) }.to_string_lossy());
        if record.field_count() > 0 {
            text.push('(');
            for index in 0..record.field_count() {
                if index > 0 {
                    text.push_str(", ");
                }
                text.push_str(&field(index));
            }
            text.push(')');
        }
    }
    new_string(text)
}

/// A hash of the variant and the fields.
#[no_mangle]
pub extern "C" fn koji_record_hash(record: *const KojiRecord) -> u64 {
    use std::hash::{Hash, Hasher};
    let record = self::record(record);
    let mut hasher = crate::hash::Fast::default();
    record.tag.hash(&mut hasher);
    for index in 0..record.field_count() {
        unsafe { (record.field_type(index).hash)(record.slot(index)) }.hash(&mut hasher);
    }
    hasher.finish()
}

/// The position of the variant `name` of `ty` (0 for an object type).
pub(crate) fn variant_index(ty: *const KojiRecordType, name: &str) -> usize {
    let ty = unsafe { &*ty };
    (0..ty.variant_count)
        .find(|&index| {
            let variant = unsafe { &*ty.variants.add(index) };
            !variant.name.is_null() && unsafe { CStr::from_ptr(variant.name) }.to_bytes() == name.as_bytes()
        })
        .unwrap_or(0)
}

/// The description of field `field` of variant `variant` of `ty`, a declared type.
pub(crate) fn field_record(ty: *const KojiRecordType, variant: &str, field: usize) -> *const KojiRecordType {
    let index = variant_index(ty, variant);
    unsafe { *(*(*ty).variants.add(index)).field_records.add(field) }
}

/// A new value of variant `name` of `ty`, which takes the references of `fields` (their words, two
/// for a decimal).
pub(crate) fn make(ty: *const KojiRecordType, name: &str, fields: &[u64]) -> *mut KojiRecord {
    let record = koji_record_new(ty, variant_index(ty, name));
    unsafe { (*record).words_mut().copy_from_slice(fields) };
    record
}

fn type_name(record: &KojiRecord) -> String {
    let name = unsafe { (*record.ty).name };
    if name.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
}

/// The text `print` shows: a readable message for an error of the standard library
/// (`'x' is not a valid number`), otherwise the value as written in code.
#[no_mangle]
pub extern "C" fn koji_record_display(record: *const KojiRecord) -> *mut KojiString {
    let value = self::record(record);
    if let Some(display) = unsafe { (*value.ty).display } {
        return unsafe { display(value) };
    }
    if let Some(variant) = unsafe { value.variant().name.as_ref() } {
        let variant = unsafe { CStr::from_ptr(variant) }.to_string_lossy();
        let texts: Vec<String> = (0..value.field_count())
            .map(|index| {
                let field = value.field_type(index);
                let text = match field.display {
                    Some(display) => unsafe { display(value.slot(index)) },
                    None => unsafe { (field.repr)(value.slot(index)) },
                };
                let result = string_text(text).to_string();
                crate::string::koji_string_release(text);
                result
            })
            .collect();
        if let Some(message) = crate::stdlib::error_message(&type_name(value), &variant, &texts) {
            return new_string(message);
        }
    }
    koji_record_repr(record)
}
