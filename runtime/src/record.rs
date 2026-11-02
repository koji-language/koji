//! Records: the values of declared types (objects and the variants of sum types, including
//! `Option` and `Result`). Like lists, they are reference-counted and copied when a shared one
//! is changed. Each field takes one 8-byte slot, described by a `KojiElementType`.

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
}

/// A declared type, as described by the generated code.
#[repr(C)]
pub struct KojiRecordType {
    pub variant_count: usize,
    pub variants: *const KojiVariantType,
}

pub struct KojiRecord {
    references: Cell<usize>,
    ty: *const KojiRecordType,
    tag: usize,
    fields: Vec<u64>,
}

impl KojiRecord {
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
        &self.fields[index] as *const u64 as *const c_void
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

    fn read(&self, index: usize, value: *const c_void) -> u64 {
        let mut slot = 0u64;
        unsafe {
            std::ptr::copy_nonoverlapping(
                value as *const u8,
                &mut slot as *mut u64 as *mut u8,
                self.field_type(index).size,
            )
        };
        slot
    }
}

fn record<'a>(record: *const KojiRecord) -> &'a KojiRecord {
    unsafe { &*record }
}

/// A new value of variant `tag` of `ty`, with its fields still empty: the generated code sets
/// each one with `koji_record_init`.
#[no_mangle]
pub extern "C" fn koji_record_new(ty: *const KojiRecordType, tag: usize) -> *mut KojiRecord {
    let count = unsafe { (*(*ty).variants.add(tag)).field_count };
    Box::into_raw(Box::new(KojiRecord {
        references: Cell::new(1),
        ty,
        tag,
        fields: vec![0; count],
    }))
}

/// Sets a field of a new record (keeping its own reference to the value).
#[no_mangle]
pub extern "C" fn koji_record_init(record: *mut KojiRecord, index: usize, value: *const c_void) {
    let record = unsafe { &mut *record };
    record.fields[index] = record.read(index, value);
    record.retain_field(index);
}

#[no_mangle]
pub extern "C" fn koji_record_retain(record: *const KojiRecord) {
    if let Some(record) = unsafe { record.as_ref() } {
        record.references.set(record.references.get() + 1);
    }
}

/// Removes a reference; at zero, releases the fields and frees the record. `NULL` is ignored.
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
    for index in 0..reference.fields.len() {
        reference.release_field(index);
    }
    drop(unsafe { Box::from_raw(record) });
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

/// Makes the record in `variable` its own (copying it if it is shared) and returns it.
fn unshare<'a>(variable: *mut *mut KojiRecord) -> &'a mut KojiRecord {
    unsafe {
        let current = &**variable;
        if current.references.get() > 1 {
            let copy = KojiRecord {
                references: Cell::new(1),
                ty: current.ty,
                tag: current.tag,
                fields: current.fields.clone(),
            };
            for index in 0..copy.fields.len() {
                copy.retain_field(index);
            }
            current.references.set(current.references.get() - 1);
            *variable = Box::into_raw(Box::new(copy));
        }
        &mut **variable
    }
}

/// The slot of a field, to change what it holds in place (`people[0].tags.push('a')`).
#[no_mangle]
pub extern "C" fn koji_record_slot(variable: *mut *mut KojiRecord, index: usize) -> *mut c_void {
    let record = unshare(variable);
    &mut record.fields[index] as *mut u64 as *mut c_void
}

/// `record.field = value`.
#[no_mangle]
pub extern "C" fn koji_record_set(variable: *mut *mut KojiRecord, index: usize, value: *const c_void) {
    let record = unshare(variable);
    let slot = record.read(index, value);
    record.release_field(index);
    record.fields[index] = slot;
    record.retain_field(index);
}

#[no_mangle]
pub extern "C" fn koji_record_equal(a: *const KojiRecord, b: *const KojiRecord) -> bool {
    let (a, b) = (record(a), record(b));
    std::ptr::eq(a, b)
        || (a.tag == b.tag
            && (0..a.fields.len()).all(|index| unsafe { (a.field_type(index).equal)(a.slot(index), b.slot(index)) }))
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
    let field = |index: usize| -> String {
        let item = unsafe { (record.field_type(index).repr)(record.slot(index)) };
        let text = string_text(item).to_string();
        crate::string::koji_string_release(item);
        text
    };
    let mut text = String::new();
    let name = record.variant().name;
    if name.is_null() {
        if record.fields.is_empty() {
            return new_string("{}".to_string());
        }
        text.push_str("{ ");
        for index in 0..record.fields.len() {
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
        if !record.fields.is_empty() {
            text.push('(');
            for index in 0..record.fields.len() {
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
