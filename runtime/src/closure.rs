//! Closures: a function with the values it captured. The generated code calls the function
//! with the environment (a record of the captured values) as its first argument.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};

use crate::list::KojiList;
use crate::record::{koji_record_release, KojiRecord};
use crate::string::{new_string, KojiString};

pub struct KojiClosure {
    references: Cell<usize>,
    code: *const c_void,
    /// The captured values, or `NULL`.
    environment: *mut KojiRecord,
    /// The function's name, for its text (`<function double>`).
    name: *const c_char,
}

fn closure<'a>(closure: *const KojiClosure) -> &'a KojiClosure {
    unsafe { &*closure }
}

/// A new closure; it takes the caller's reference to `environment`.
#[no_mangle]
pub extern "C" fn koji_closure_new(code: *const c_void, environment: *mut KojiRecord, name: *const c_char) -> *mut KojiClosure {
    Box::into_raw(Box::new(KojiClosure {
        references: Cell::new(1),
        code,
        environment,
        name,
    }))
}

#[no_mangle]
pub extern "C" fn koji_closure_retain(closure: *const KojiClosure) {
    if let Some(closure) = unsafe { closure.as_ref() } {
        closure.references.set(closure.references.get() + 1);
    }
}

#[no_mangle]
pub extern "C" fn koji_closure_release(closure: *mut KojiClosure) {
    let Some(reference) = (unsafe { closure.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    koji_record_release(reference.environment);
    drop(unsafe { Box::from_raw(closure) });
}

#[no_mangle]
pub extern "C" fn koji_closure_code(closure: *const KojiClosure) -> *const c_void {
    self::closure(closure).code
}

#[no_mangle]
pub extern "C" fn koji_closure_environment(closure: *const KojiClosure) -> *mut KojiRecord {
    self::closure(closure).environment
}

/// `<function double>`.
#[no_mangle]
pub extern "C" fn koji_closure_repr(closure: *const KojiClosure) -> *mut KojiString {
    let name = unsafe { CStr::from_ptr(self::closure(closure).name) }.to_string_lossy();
    new_string(format!("<function {}>", name))
}

/// The same closure (functions have no other equality).
#[no_mangle]
pub extern "C" fn koji_closure_equal(a: *const KojiClosure, b: *const KojiClosure) -> bool {
    std::ptr::eq(a, b)
}

/// `items.sortBy(comparison)`: a stable merge sort, where `call(comparison, a, b)` says whether
/// `a` goes before `b` (the generated code knows the type of the items).
#[no_mangle]
pub extern "C" fn koji_list_sort_by(
    variable: *mut *mut KojiList,
    comparison: *const KojiClosure,
    call: unsafe extern "C" fn(*const KojiClosure, *const c_void, *const c_void) -> bool,
) {
    let list = crate::list::unshare(variable);
    let items = std::mem::take(&mut list.items);
    list.items = merge_sort(items, &|a: &u64, b: &u64| unsafe {
        call(comparison, a as *const u64 as *const c_void, b as *const u64 as *const c_void)
    });
}

/// A stable sort: an item from the right half goes first only when it goes strictly
/// before, so equal items keep their order.
fn merge_sort(items: Vec<u64>, before: &dyn Fn(&u64, &u64) -> bool) -> Vec<u64> {
    if items.len() <= 1 {
        return items;
    }
    let mut left = items;
    let right = left.split_off(left.len() / 2);
    let left = merge_sort(left, before);
    let right = merge_sort(right, before);
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut left, mut right) = (left.into_iter().peekable(), right.into_iter().peekable());
    while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
        let right_first = before(b, a);
        merged.push(if right_first { right.next() } else { left.next() }.expect("peeked"));
    }
    merged.extend(left);
    merged.extend(right);
    merged
}
