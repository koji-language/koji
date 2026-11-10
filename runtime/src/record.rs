//! The layout of records, for the runtime in Rust that still makes them (`http.rs`, which builds
//! its `Result`): records are in std/koji/record.kj and in the prelude.

use std::ffi::{c_char, c_void, CStr};

use crate::string::KojiString;

/// How the generated code describes the items of a list, a map or a record (runtime/prelude.h).
#[repr(C)]
pub struct KojiElementType {
    pub size: usize,
    pub retain: Option<unsafe extern "C" fn(*const c_void)>,
    pub release: Option<unsafe extern "C" fn(*const c_void)>,
    pub repr: unsafe extern "C" fn(*const c_void) -> *mut KojiString,
    pub equal: unsafe extern "C" fn(*const c_void, *const c_void) -> bool,
    pub compare: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> i32>,
    pub hash: unsafe extern "C" fn(*const c_void) -> u64,
    pub display: Option<unsafe extern "C" fn(*const c_void) -> *mut KojiString>,
}

/// One variant of a sum type, or the fields of an object type.
#[repr(C)]
pub struct KojiVariantType {
    pub name: *const c_char,
    pub field_count: usize,
    pub field_names: *const *const c_char,
    pub field_types: *const *const KojiElementType,
    /// For each field of a declared type, its description (`NULL` for the others).
    pub field_records: *const *const KojiRecordType,
    /// Where each field starts, in words, then the number of words (`NULL` when every field
    /// takes one word).
    pub field_offsets: *const usize,
}

impl KojiVariantType {
    fn word_count(&self) -> usize {
        if self.field_offsets.is_null() {
            self.field_count
        } else {
            unsafe { *self.field_offsets.add(self.field_count) }
        }
    }
}

/// A declared type, as described by the generated code.
#[repr(C)]
pub struct KojiRecordType {
    pub name: *const c_char,
    pub variant_count: usize,
    pub variants: *const KojiVariantType,
    pub free: Option<unsafe extern "C" fn(*mut KojiRecord)>,
    pub display: Option<unsafe extern "C" fn(*const KojiRecord) -> *mut KojiString>,
}

/// A record: 3 words, then the words of its fields.
#[repr(C)]
pub struct KojiRecord {
    references: usize,
    ty: *const KojiRecordType,
    tag: usize,
    fields: [u64; 0],
}

extern "C" {
    /// A block of the pool (std/koji/memory.kj).
    fn koji_pool_take_block(words: usize) -> *mut u8;
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

/// A new value of variant `name` of `ty`, in a block of the pool as the prelude makes them, which
/// takes the references of `fields` (their words, two for a decimal).
pub(crate) fn make(ty: *const KojiRecordType, name: &str, fields: &[u64]) -> *mut KojiRecord {
    let tag = variant_index(ty, name);
    let words = unsafe { (*(*ty).variants.add(tag)).word_count() };
    assert_eq!(words, fields.len(), "the words of a record");
    unsafe {
        let record = koji_pool_take_block(3 + words) as *mut KojiRecord;
        record.write(KojiRecord { references: 1, ty, tag, fields: [] });
        std::ptr::copy_nonoverlapping(fields.as_ptr(), (*record).fields.as_mut_ptr(), words);
        record
    }
}
