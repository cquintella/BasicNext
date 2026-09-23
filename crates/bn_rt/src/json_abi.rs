// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // Fallible helpers return status/`Result` documented per call site.
#![allow(clippy::must_use_candidate)] // i32 status codes are checked at every BNJson call site.
#![allow(clippy::doc_markdown)] // Status constant names appear bare in prose.
#![allow(clippy::double_must_use)]

//! The `BNJson` document table and its C ABI. One table serves both backends:
//! `bn_lib_json` marshals `Value`s into these calls and the LLVM backend emits
//! them directly, so a handle means the same thing interpreted and compiled.
//!
//! Bounds are enforced where they are breached. A `set` that would push a
//! document past the depth limit fails at that call rather than leaving a
//! document that only `stringify` will later refuse.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::json::{MAX_DEPTH, parse, stringify};
use crate::{c_str, c_string};

pub const BN_JSON_OK: i32 = 0;
pub const BN_JSON_INVALID_ARGUMENT: i32 = 1;
pub const BN_JSON_INVALID_HANDLE: i32 = 2;
/// The key is absent, or holds a value of another type. Callers turn this into
/// an `Error`, never into a default value.
pub const BN_JSON_NOT_FOUND: i32 = 3;
/// The operation would breach the depth or size bound.
pub const BN_JSON_TOO_LARGE: i32 = 4;

fn documents() -> &'static Mutex<HashMap<u64, serde_json::Value>> {
    static DOCUMENTS: OnceLock<Mutex<HashMap<u64, serde_json::Value>>> = OnceLock::new();
    DOCUMENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_handle() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn with_documents<T>(operation: impl FnOnce(&mut HashMap<u64, serde_json::Value>) -> T) -> T {
    operation(&mut documents().lock().unwrap_or_else(PoisonError::into_inner))
}

/// Files `value` and returns its handle. Rust callers inside the interpreter
/// use this directly; compiled code reaches it through the ABI below.
#[must_use]
pub fn store(value: serde_json::Value) -> u64 {
    let handle = next_handle();
    with_documents(|documents| documents.insert(handle, value));
    handle
}

/// A clone of the document behind `handle`.
#[must_use]
pub fn document(handle: u64) -> Option<serde_json::Value> {
    with_documents(|documents| documents.get(&handle).cloned())
}

/// Drops `handle`. `false` when it was already released.
pub fn release(handle: u64) -> bool {
    with_documents(|documents| documents.remove(&handle).is_some())
}

/// Depth of `value`, counting the value itself as one level.
fn depth_of(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(entries) => 1 + entries.values().map(depth_of).max().unwrap_or(0),
        serde_json::Value::Array(items) => 1 + items.iter().map(depth_of).max().unwrap_or(0),
        _ => 1,
    }
}

/// Writes a STRING member, failing closed on a non-object target and on a
/// write that would breach the depth bound.
pub fn set_string(handle: u64, key: &str, value: &str) -> i32 {
    set_value(handle, key, serde_json::Value::String(value.to_owned()))
}

/// Reads a STRING member. A missing key and a key of another type are both
/// [`BN_JSON_NOT_FOUND`]; neither yields an empty string.
pub fn get_string(handle: u64, key: &str) -> Result<String, i32> {
    get_value(handle, key, |value| value.as_str().map(str::to_owned))
}

/// Public wrapper so the interpreter provider reaches the same write path the
/// C ABI uses, rather than keeping a second copy of the bound check.
pub fn set_value_public(handle: u64, key: &str, value: serde_json::Value) -> i32 {
    set_value(handle, key, value)
}

/// Writes a member of any scalar kind, sharing one bound check and one
/// non-object rejection with [`set_string`].
fn set_value(handle: u64, key: &str, value: serde_json::Value) -> i32 {
    with_documents(|documents| {
        let Some(target) = documents.get_mut(&handle) else {
            return BN_JSON_INVALID_HANDLE;
        };
        let Some(object) = target.as_object_mut() else {
            return BN_JSON_INVALID_ARGUMENT;
        };
        object.insert(key.to_owned(), value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(object) = target.as_object_mut() {
                object.remove(key);
            }
            return BN_JSON_TOO_LARGE;
        }
        BN_JSON_OK
    })
}

/// Reads a member and projects it with `project`. A missing key and a key of
/// another kind are both [`BN_JSON_NOT_FOUND`]; neither yields a default.
fn get_value<T>(
    handle: u64,
    key: &str,
    project: impl Fn(&serde_json::Value) -> Option<T>,
) -> Result<T, i32> {
    with_documents(|documents| {
        let Some(target) = documents.get(&handle) else {
            return Err(BN_JSON_INVALID_HANDLE);
        };
        target.get(key).and_then(project).ok_or(BN_JSON_NOT_FOUND)
    })
}

/// The kind of a document: `object`, `array`, `string`, `number`, `boolean` or
/// `null`. Queryable directly rather than guessed from `stringify` output.
#[must_use]
pub fn kind(handle: u64) -> Option<&'static str> {
    with_documents(|documents| {
        documents.get(&handle).map(|value| match value {
            serde_json::Value::Object(_) => "object",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Null => "null",
        })
    })
}

/// Whether `key` is present, regardless of the value's kind.
#[must_use]
pub fn has(handle: u64, key: &str) -> bool {
    with_documents(|documents| {
        documents
            .get(&handle)
            .is_some_and(|value| value.get(key).is_some())
    })
}

/// Member count for an object, element count for an array. `-1` for an unknown
/// handle or a scalar, which has no length.
#[must_use]
pub fn length(handle: u64) -> i64 {
    with_documents(|documents| {
        documents
            .get(&handle)
            .and_then(|value| match value {
                serde_json::Value::Object(entries) => i64::try_from(entries.len()).ok(),
                serde_json::Value::Array(items) => i64::try_from(items.len()).ok(),
                _ => None,
            })
            .unwrap_or(-1)
    })
}

/// A new, empty array.
#[allow(unsafe_code)] // C ABI: no input, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_array() -> u64 {
    store(serde_json::Value::Array(Vec::new()))
}

/// Writes an INTEGER member.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_integer(handle: u64, key: *const c_char, value: i64) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_value(handle, key, serde_json::Value::from(value))
}

/// Writes a BOOLEAN member.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_boolean(handle: u64, key: *const c_char, value: i32) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_value(handle, key, serde_json::Value::Bool(value != 0))
}

/// Writes a NULL member. Present-and-null is distinct from absent, and `Has`
/// reports the difference.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_null(handle: u64, key: *const c_char) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_value(handle, key, serde_json::Value::Null)
}

/// Reads an INTEGER member, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_integer(handle: u64, key: *const c_char, status: *mut i32) -> i64 {
    if status.is_null() {
        return 0;
    }
    let Some(key) = c_str(key) else {
        unsafe { *status = BN_JSON_INVALID_ARGUMENT };
        return 0;
    };
    match get_value(handle, key, serde_json::Value::as_i64) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            value
        }
        Err(code) => {
            unsafe { *status = code };
            0
        }
    }
}

/// Reads a BOOLEAN member as `0` or `1`, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_boolean(handle: u64, key: *const c_char, status: *mut i32) -> i32 {
    if status.is_null() {
        return 0;
    }
    let Some(key) = c_str(key) else {
        unsafe { *status = BN_JSON_INVALID_ARGUMENT };
        return 0;
    };
    match get_value(handle, key, serde_json::Value::as_bool) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            i32::from(value)
        }
        Err(code) => {
            unsafe { *status = code };
            0
        }
    }
}

/// The document's kind as an owned string; `""` for an unknown handle.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_kind(handle: u64) -> *mut c_char {
    c_string(kind(handle).unwrap_or_default())
}

/// Whether `key` is present. `0` or `1`.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, boolean out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_has(handle: u64, key: *const c_char) -> i32 {
    let Some(key) = c_str(key) else {
        return 0;
    };
    i32::from(has(handle, key))
}

/// Member or element count; `-1` when the handle is unknown or scalar.
#[allow(unsafe_code)] // C ABI: opaque handle in, count out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_length(handle: u64) -> i64 {
    length(handle)
}

/// Appends `value` to an array, failing closed on a non-array target and on an
/// append that would breach the depth bound.
fn append_value(handle: u64, value: serde_json::Value) -> i32 {
    with_documents(|documents| {
        let Some(target) = documents.get_mut(&handle) else {
            return BN_JSON_INVALID_HANDLE;
        };
        let Some(items) = target.as_array_mut() else {
            return BN_JSON_INVALID_ARGUMENT;
        };
        items.push(value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(items) = target.as_array_mut() {
                items.pop();
            }
            return BN_JSON_TOO_LARGE;
        }
        BN_JSON_OK
    })
}

/// Reads element `index` and projects it. An out-of-range index and an element
/// of another kind are both [`BN_JSON_NOT_FOUND`].
fn element<T>(
    handle: u64,
    index: i64,
    project: impl Fn(&serde_json::Value) -> Option<T>,
) -> Result<T, i32> {
    with_documents(|documents| {
        let Some(target) = documents.get(&handle) else {
            return Err(BN_JSON_INVALID_HANDLE);
        };
        let Ok(index) = usize::try_from(index) else {
            return Err(BN_JSON_NOT_FOUND);
        };
        target.get(index).and_then(project).ok_or(BN_JSON_NOT_FOUND)
    })
}

/// Public append for the interpreter provider, mirroring [`set_value_public`].
pub fn append_value_public(handle: u64, value: serde_json::Value) -> i32 {
    append_value(handle, value)
}

/// Public element read for the interpreter provider.
#[must_use]
pub fn element_at(handle: u64, index: i64) -> Option<serde_json::Value> {
    element(handle, index, |value| Some(value.clone())).ok()
}

/// Duplicates a document into a new handle. Nesting moves rather than copies,
/// so duplication is always something the caller asked for by name.
#[must_use]
pub fn clone_document(handle: u64) -> Option<u64> {
    document(handle).map(store)
}

/// Moves `child` into `parent` under `key`. The child handle is **consumed**:
/// a value lives in exactly one place, so there is no aliasing, no cycle can be
/// built, and the depth check below sees an inert child.
pub fn move_into(parent: u64, key: &str, child: u64) -> i32 {
    if parent == child {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(value) = document(child) else {
        return BN_JSON_INVALID_HANDLE;
    };
    let code = set_value(parent, key, value);
    if code == BN_JSON_OK {
        release(child);
    }
    code
}

/// Writes a FLOAT member. A non-finite number is rejected, matching the 0.3
/// parse rules — JSON has no NaN or infinity.
pub fn set_float(handle: u64, key: &str, value: f64) -> i32 {
    let Some(number) = serde_json::Number::from_f64(value) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_value(handle, key, serde_json::Value::Number(number))
}

/// A new, empty object. Returns its handle.
#[allow(unsafe_code)] // C ABI: no input, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_object() -> u64 {
    store(serde_json::Value::Object(serde_json::Map::new()))
}

/// Parses `text` under the 0.3 bounds, writing the handle through `out`.
#[allow(unsafe_code)] // C ABI: STRING in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_parse(text: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(text) = c_str(text) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    let Ok(value) = parse(text) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    let handle = store(value);
    unsafe { *out = handle };
    BN_JSON_OK
}

/// Serializes a document, writing the status through `status`. The returned
/// string is owned by the caller and is `""` on anything but [`BN_JSON_OK`] —
/// callers must branch on the status, not on the string, because `""` is not
/// valid JSON and must never be mistaken for a result.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_stringify(handle: u64, status: *mut i32) -> *mut c_char {
    if status.is_null() {
        return c_string("");
    }
    let Some(value) = document(handle) else {
        unsafe { *status = BN_JSON_INVALID_HANDLE };
        return c_string("");
    };
    match stringify(&value) {
        Ok(text) => {
            unsafe { *status = BN_JSON_OK };
            c_string(&text)
        }
        Err(_) => {
            unsafe { *status = BN_JSON_TOO_LARGE };
            c_string("")
        }
    }
}

/// Writes a STRING member under `key`.
#[allow(unsafe_code)] // C ABI: opaque handle plus two STRINGs in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_string(
    handle: u64,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    let (Some(key), Some(value)) = (c_str(key), c_str(value)) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_string(handle, key, value)
}

/// Reads a STRING member into a freshly allocated string, writing the status
/// through `status`. On anything but [`BN_JSON_OK`] the returned string is `""`
/// and must not be read as a value.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_string(
    handle: u64,
    key: *const c_char,
    status: *mut i32,
) -> *mut c_char {
    if status.is_null() {
        return c_string("");
    }
    let Some(key) = c_str(key) else {
        unsafe { *status = BN_JSON_INVALID_ARGUMENT };
        return c_string("");
    };
    match get_string(handle, key) {
        Ok(text) => {
            unsafe { *status = BN_JSON_OK };
            c_string(&text)
        }
        Err(code) => {
            unsafe { *status = code };
            c_string("")
        }
    }
}

/// Writes a FLOAT member; a non-finite value is rejected.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_float(handle: u64, key: *const c_char, value: f64) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_float(handle, key, value)
}

/// Reads a FLOAT member, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_float(handle: u64, key: *const c_char, status: *mut i32) -> f64 {
    if status.is_null() {
        return 0.0;
    }
    let Some(key) = c_str(key) else {
        unsafe { *status = BN_JSON_INVALID_ARGUMENT };
        return 0.0;
    };
    match get_value(handle, key, serde_json::Value::as_f64) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            value
        }
        Err(code) => {
            unsafe { *status = code };
            0.0
        }
    }
}

/// Appends a STRING to an array.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_string(handle: u64, value: *const c_char) -> i32 {
    let Some(value) = c_str(value) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    append_value(handle, serde_json::Value::String(value.to_owned()))
}

/// Appends an INTEGER to an array.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_integer(handle: u64, value: i64) -> i32 {
    append_value(handle, serde_json::Value::from(value))
}

/// Reads a STRING element, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_string_at(
    handle: u64,
    index: i64,
    status: *mut i32,
) -> *mut c_char {
    if status.is_null() {
        return c_string("");
    }
    match element(handle, index, |value| value.as_str().map(str::to_owned)) {
        Ok(text) => {
            unsafe { *status = BN_JSON_OK };
            c_string(&text)
        }
        Err(code) => {
            unsafe { *status = code };
            c_string("")
        }
    }
}

/// Duplicates a document, writing the new handle through `out`.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_clone(handle: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(copy) = clone_document(handle) else {
        return BN_JSON_INVALID_HANDLE;
    };
    unsafe { *out = copy };
    BN_JSON_OK
}

/// Moves `child` into `parent` under `key`, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_json(parent: u64, key: *const c_char, child: u64) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    move_into(parent, key, child)
}

/// Writes element `index` of an array. Out-of-range is [`BN_JSON_NOT_FOUND`]; a
/// non-array target is [`BN_JSON_INVALID_ARGUMENT`]. The depth bound is checked
/// at the write, same as [`set_value`].
fn set_at(handle: u64, index: i64, value: serde_json::Value) -> i32 {
    with_documents(|documents| {
        let Some(target) = documents.get_mut(&handle) else {
            return BN_JSON_INVALID_HANDLE;
        };
        let Some(items) = target.as_array_mut() else {
            return BN_JSON_INVALID_ARGUMENT;
        };
        let Ok(index) = usize::try_from(index) else {
            return BN_JSON_NOT_FOUND;
        };
        if index >= items.len() {
            return BN_JSON_NOT_FOUND;
        }
        let previous = std::mem::replace(&mut items[index], value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(items) = target.as_array_mut() {
                items[index] = previous;
            }
            return BN_JSON_TOO_LARGE;
        }
        BN_JSON_OK
    })
}

/// Public array write for the interpreter provider.
pub fn set_at_public(handle: u64, index: i64, value: serde_json::Value) -> i32 {
    set_at(handle, index, value)
}

/// Moves `child` into `parent` at array `index`, consuming the child handle.
pub fn move_into_at(parent: u64, index: i64, child: u64) -> i32 {
    if parent == child {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(value) = document(child) else {
        return BN_JSON_INVALID_HANDLE;
    };
    let code = set_at(parent, index, value);
    if code == BN_JSON_OK {
        release(child);
    }
    code
}

/// Appends `child` onto an array, consuming the child handle (AppendJson twin of
/// SetJson's move).
pub fn append_moved(parent: u64, child: u64) -> i32 {
    if parent == child {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(value) = document(child) else {
        return BN_JSON_INVALID_HANDLE;
    };
    let code = append_value(parent, value);
    if code == BN_JSON_OK {
        release(child);
    }
    code
}

/// A fresh handle to the member under `key`. The parent is not consumed; the
/// caller owns the new handle and must `RELEASE` it.
pub fn get_json(handle: u64, key: &str) -> Result<u64, i32> {
    let value = with_documents(|documents| {
        let Some(target) = documents.get(&handle) else {
            return Err(BN_JSON_INVALID_HANDLE);
        };
        target.get(key).cloned().ok_or(BN_JSON_NOT_FOUND)
    })?;
    Ok(store(value))
}

/// A fresh handle to array element `index`. The parent is not consumed.
pub fn get_json_at(handle: u64, index: i64) -> Result<u64, i32> {
    let value = element(handle, index, |value| Some(value.clone()))?;
    Ok(store(value))
}

/// Appends a BOOLEAN to an array.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_boolean(handle: u64, value: i32) -> i32 {
    append_value(handle, serde_json::Value::Bool(value != 0))
}

/// Appends a FLOAT to an array. Non-finite values are rejected.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_float(handle: u64, value: f64) -> i32 {
    let Some(number) = serde_json::Number::from_f64(value) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    append_value(handle, serde_json::Value::Number(number))
}

/// Appends NULL to an array.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_null(handle: u64) -> i32 {
    append_value(handle, serde_json::Value::Null)
}

/// Appends `child` onto an array, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_json(parent: u64, child: u64) -> i32 {
    append_moved(parent, child)
}

/// Reads an INTEGER element, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_integer_at(handle: u64, index: i64, status: *mut i32) -> i64 {
    if status.is_null() {
        return 0;
    }
    match element(handle, index, serde_json::Value::as_i64) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            value
        }
        Err(code) => {
            unsafe { *status = code };
            0
        }
    }
}

/// Reads a BOOLEAN element as `0` or `1`, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_boolean_at(handle: u64, index: i64, status: *mut i32) -> i32 {
    if status.is_null() {
        return 0;
    }
    match element(handle, index, serde_json::Value::as_bool) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            i32::from(value)
        }
        Err(code) => {
            unsafe { *status = code };
            0
        }
    }
}

/// Reads a FLOAT element, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_float_at(handle: u64, index: i64, status: *mut i32) -> f64 {
    if status.is_null() {
        return 0.0;
    }
    match element(handle, index, serde_json::Value::as_f64) {
        Ok(value) => {
            unsafe { *status = BN_JSON_OK };
            value
        }
        Err(code) => {
            unsafe { *status = code };
            0.0
        }
    }
}

/// A fresh handle to element `index`, writing it through `out`.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_json_at(handle: u64, index: i64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_JSON_INVALID_ARGUMENT;
    }
    match get_json_at(handle, index) {
        Ok(copy) => {
            unsafe { *out = copy };
            BN_JSON_OK
        }
        Err(code) => code,
    }
}

/// A fresh handle to the member under `key`, writing it through `out`. The
/// parent is not consumed; the caller owns the new handle.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_json(handle: u64, key: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_JSON_INVALID_ARGUMENT;
    }
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    match get_json(handle, key) {
        Ok(copy) => {
            unsafe { *out = copy };
            BN_JSON_OK
        }
        Err(code) => code,
    }
}

/// Writes a STRING element at `index`.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_string_at(handle: u64, index: i64, value: *const c_char) -> i32 {
    let Some(value) = c_str(value) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_at(handle, index, serde_json::Value::String(value.to_owned()))
}

/// Writes an INTEGER element at `index`.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_integer_at(handle: u64, index: i64, value: i64) -> i32 {
    set_at(handle, index, serde_json::Value::from(value))
}

/// Writes a BOOLEAN element at `index`.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_boolean_at(handle: u64, index: i64, value: i32) -> i32 {
    set_at(handle, index, serde_json::Value::Bool(value != 0))
}

/// Writes a FLOAT element at `index`. Non-finite values are rejected.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_float_at(handle: u64, index: i64, value: f64) -> i32 {
    let Some(number) = serde_json::Number::from_f64(value) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    set_at(handle, index, serde_json::Value::Number(number))
}

/// Writes a NULL element at `index`.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_null_at(handle: u64, index: i64) -> i32 {
    set_at(handle, index, serde_json::Value::Null)
}

/// Moves `child` into the array at `index`, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_json_at(parent: u64, index: i64, child: u64) -> i32 {
    move_into_at(parent, index, child)
}

/// Drops a document. [`BN_JSON_INVALID_HANDLE`] when it was already released,
/// so a double `RELEASE` is reported rather than ignored.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_release(handle: u64) -> i32 {
    if release(handle) {
        BN_JSON_OK
    } else {
        BN_JSON_INVALID_HANDLE
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BN_JSON_INVALID_HANDLE, BN_JSON_NOT_FOUND, BN_JSON_OK, bn_rt_json_object, document,
        get_string, release, set_string,
    };

    #[test]
    fn object_round_trips_and_fails_closed() {
        let handle = bn_rt_json_object();
        assert_eq!(set_string(handle, "name", "pardal"), BN_JSON_OK);
        assert_eq!(get_string(handle, "name").as_deref(), Ok("pardal"));
        // A missing key is an error, never an empty string.
        assert_eq!(get_string(handle, "absent"), Err(BN_JSON_NOT_FOUND));
        assert!(document(handle).is_some());
        assert!(release(handle));
        // Released once, gone for good.
        assert!(!release(handle));
        assert_eq!(get_string(handle, "name"), Err(BN_JSON_INVALID_HANDLE));
    }

    #[test]
    fn nesting_moves_the_child_and_cannot_build_a_cycle() {
        use super::{bn_rt_json_array, clone_document, document, move_into};
        let parent = bn_rt_json_object();
        let child = bn_rt_json_object();
        assert_eq!(set_string(child, "k", "v"), BN_JSON_OK);

        assert_eq!(move_into(parent, "nest", child), BN_JSON_OK);
        // The child handle is consumed: a value lives in exactly one place.
        assert!(document(child).is_none());
        assert_eq!(move_into(parent, "again", child), BN_JSON_INVALID_HANDLE);
        // The value itself survived, inside the parent.
        assert_eq!(get_string(parent, "k"), Err(BN_JSON_NOT_FOUND));
        assert!(document(parent).is_some_and(|value| value.get("nest").is_some()));

        // The shortest possible cycle is refused outright.
        let solo = bn_rt_json_object();
        assert_eq!(
            move_into(solo, "self", solo),
            super::BN_JSON_INVALID_ARGUMENT
        );

        // Clone is the explicit way to duplicate, and the copy is independent.
        let original = bn_rt_json_array();
        let copy = clone_document(original).expect("clone");
        assert_ne!(original, copy);
        assert!(release(original));
        assert!(document(copy).is_some());
    }

    #[test]
    fn handles_are_distinct() {
        let first = bn_rt_json_object();
        let second = bn_rt_json_object();
        assert_ne!(first, second);
        assert_eq!(set_string(first, "k", "1"), BN_JSON_OK);
        assert_eq!(get_string(second, "k"), Err(BN_JSON_NOT_FOUND));
    }

    #[test]
    fn float_set_get_and_rejects_non_finite() {
        use super::{bn_rt_json_get_float, set_float};
        use std::ffi::CString;
        let handle = bn_rt_json_object();
        assert_eq!(set_float(handle, "pi", 3.5), BN_JSON_OK);
        let key = CString::new("pi").unwrap();
        let mut status = -1;
        let value = bn_rt_json_get_float(handle, key.as_ptr(), &raw mut status);
        assert_eq!(status, BN_JSON_OK);
        assert!((value - 3.5).abs() < f64::EPSILON);
        assert_eq!(
            set_float(handle, "bad", f64::NAN),
            super::BN_JSON_INVALID_ARGUMENT
        );
        assert_eq!(set_string(handle, "name", "x"), BN_JSON_OK);
        let name = CString::new("name").unwrap();
        let mut status = -1;
        let _ = bn_rt_json_get_float(handle, name.as_ptr(), &raw mut status);
        assert_eq!(status, BN_JSON_NOT_FOUND);
        assert!(release(handle));
    }

    #[test]
    fn array_append_index_and_oob_fail_closed() {
        use super::{
            append_value, bn_rt_json_array, bn_rt_json_get_string_at, get_json_at, set_at,
        };
        let arr = bn_rt_json_array();
        assert_eq!(
            append_value(arr, serde_json::Value::String("a".into())),
            BN_JSON_OK
        );
        assert_eq!(append_value(arr, serde_json::Value::from(7i64)), BN_JSON_OK);
        let mut status = -1;
        let _text = bn_rt_json_get_string_at(arr, 0, &raw mut status);
        assert_eq!(status, BN_JSON_OK);
        let mut status = -1;
        let _ = bn_rt_json_get_string_at(arr, 99, &raw mut status);
        assert_eq!(status, BN_JSON_NOT_FOUND);
        assert_eq!(set_at(arr, 1, serde_json::Value::from(8i64)), BN_JSON_OK);
        assert_eq!(set_at(arr, 99, serde_json::Value::Null), BN_JSON_NOT_FOUND);
        let nested = get_json_at(arr, 0).expect("element handle");
        assert!(document(nested).is_some());
        assert!(release(nested));
        assert!(release(arr));
    }

    #[test]
    fn get_json_clones_nested_without_consuming_parent() {
        use super::{get_json, move_into};
        let parent = bn_rt_json_object();
        let child = bn_rt_json_object();
        assert_eq!(set_string(child, "k", "v"), BN_JSON_OK);
        assert_eq!(move_into(parent, "nest", child), BN_JSON_OK);
        let copy = get_json(parent, "nest").expect("nested clone");
        assert!(document(parent).is_some());
        assert_eq!(get_string(copy, "k").as_deref(), Ok("v"));
        assert!(release(copy));
        let again = get_json(parent, "nest").expect("still there");
        assert!(release(again));
        assert!(release(parent));
    }
}
