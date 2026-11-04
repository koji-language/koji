//! The `file` module: files (move-only resources, closed when dropped) and directories.

use std::cell::{Cell, RefCell};
use std::ffi::c_char;
use std::io::{Read, Write};

use crate::list::{new_list, KojiElementType};
use crate::output::panic_at;
use crate::record::{field_record, make, record_variant, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

pub struct KojiFile {
    references: Cell<usize>,
    path: String,
    /// `Read`, `Write` or `Append`.
    mode: String,
    /// `None` once closed.
    file: RefCell<Option<std::fs::File>>,
}

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

/// `Err(FileError)` for an error of the operating system.
fn file_error(result: *const KojiRecordType, path: &str, error: &std::io::Error) -> *mut KojiRecord {
    use std::io::ErrorKind;
    let error_type = field_record(result, "Err", 0);
    let path_slot = slot(new_string(path.to_string()));
    let value = match error.kind() {
        ErrorKind::NotFound => make(error_type, "NotFound", &[path_slot]),
        ErrorKind::PermissionDenied => make(error_type, "PermissionDenied", &[path_slot]),
        ErrorKind::IsADirectory => make(error_type, "IsDirectory", &[path_slot]),
        ErrorKind::InvalidData => make(error_type, "InvalidUtf8", &[path_slot]),
        _ => make(error_type, "Io", &[path_slot, slot(new_string(error.to_string()))]),
    };
    make(result, "Err", &[value as usize as u64])
}

/// `file.open(path, mode)`: `Ok(File)` or `Err(FileError)`.
#[no_mangle]
pub extern "C" fn koji_std_file_open(path: *const KojiString, mode: *const KojiRecord, result: *const KojiRecordType) -> *mut KojiRecord {
    let path = string_text(path).to_string();
    let mode = record_variant(mode);
    let mut options = std::fs::OpenOptions::new();
    match mode.as_str() {
        "Read" => options.read(true),
        "Write" => options.write(true).create(true).truncate(true),
        _ => options.append(true).create(true),
    };
    match options.open(&path) {
        Ok(file) => {
            let handle = Box::into_raw(Box::new(KojiFile {
                references: Cell::new(1),
                path,
                mode,
                file: RefCell::new(Some(file)),
            }));
            make(result, "Ok", &[handle as usize as u64])
        }
        Err(error) => file_error(result, &path, &error),
    }
}

#[no_mangle]
pub extern "C" fn koji_file_retain(file: *const KojiFile) {
    if let Some(file) = unsafe { file.as_ref() } {
        file.references.set(file.references.get() + 1);
    }
}

#[no_mangle]
pub extern "C" fn koji_file_release(file: *mut KojiFile) {
    let Some(reference) = (unsafe { file.as_ref() }) else {
        return;
    };
    let references = reference.references.get() - 1;
    if references > 0 {
        reference.references.set(references);
        return;
    }
    drop(unsafe { Box::from_raw(file) });
}

/// `file.close()`, and what dropping a file does.
#[no_mangle]
pub extern "C" fn koji_std_file_close(file: *const KojiFile) {
    unsafe { &*file }.file.borrow_mut().take();
}

/// `<file 'notes.txt'>`.
#[no_mangle]
pub extern "C" fn koji_file_repr(file: *const KojiFile) -> *mut KojiString {
    new_string(format!("<file '{}'>", unsafe { &*file }.path))
}

/// `file.read()`: `Ok(text)` or `Err(FileError)`.
#[no_mangle]
pub extern "C" fn koji_std_file_read(file: *const KojiFile, result: *const KojiRecordType, location: *const c_char) -> *mut KojiRecord {
    let handle = unsafe { &*file };
    let mut file = handle.file.borrow_mut();
    let Some(file) = file.as_mut() else {
        panic_at(location, &format!("file '{}' is closed", handle.path));
    };
    if handle.mode != "Read" {
        panic_at(location, &format!("file '{}' was not opened for reading (Read)", handle.path));
    }
    let mut content = String::new();
    match file.read_to_string(&mut content) {
        Ok(_) => make(result, "Ok", &[slot(new_string(content))]),
        Err(error) => file_error(result, &handle.path, &error),
    }
}

/// `file.write(text)`: `Ok(void)` or `Err(FileError)`.
#[no_mangle]
pub extern "C" fn koji_std_file_write(file: *const KojiFile, text: *const KojiString, result: *const KojiRecordType, location: *const c_char) -> *mut KojiRecord {
    let handle = unsafe { &*file };
    let mut file = handle.file.borrow_mut();
    let Some(file) = file.as_mut() else {
        panic_at(location, &format!("file '{}' is closed", handle.path));
    };
    if handle.mode == "Read" {
        panic_at(location, &format!("file '{}' was opened for reading; use Write or Append to write", handle.path));
    }
    match file.write_all(string_text(text).as_bytes()) {
        Ok(()) => make(result, "Ok", &[0]),
        Err(error) => file_error(result, &handle.path, &error),
    }
}

/// `file.list(path)`: the names of the entries, sorted.
#[no_mangle]
pub extern "C" fn koji_std_file_list(path: *const KojiString, element: *const KojiElementType, result: *const KojiRecordType) -> *mut KojiRecord {
    let path = string_text(path).to_string();
    let entries = std::fs::read_dir(&path).and_then(|entries| {
        entries
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_string()))
            .collect::<std::io::Result<Vec<String>>>()
    });
    match entries {
        Ok(mut names) => {
            names.sort();
            let list = new_list(element, names.into_iter().map(|name| slot(new_string(name))).collect());
            make(result, "Ok", &[list as usize as u64])
        }
        Err(error) => file_error(result, &path, &error),
    }
}

#[no_mangle]
pub extern "C" fn koji_std_file_exists(path: *const KojiString) -> bool {
    std::path::Path::new(string_text(path)).exists()
}

/// `file.remove(path)`: removes a file (not a directory).
#[no_mangle]
pub extern "C" fn koji_std_file_remove(path: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    let path = string_text(path).to_string();
    match std::fs::remove_file(&path) {
        Ok(()) => make(result, "Ok", &[0]),
        Err(error) => file_error(result, &path, &error),
    }
}

/// `file.createDir(path)`, with the missing parent directories.
#[no_mangle]
pub extern "C" fn koji_std_file_create_dir(path: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    let path = string_text(path).to_string();
    match std::fs::create_dir_all(&path) {
        Ok(()) => make(result, "Ok", &[0]),
        Err(error) => file_error(result, &path, &error),
    }
}
