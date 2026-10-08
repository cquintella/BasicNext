// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // Every failure is a `JsonFailure` (json_error.rs).
#![allow(clippy::doc_markdown)] // Status constant names appear bare in prose.
#![allow(clippy::must_use_candidate)] // Statuses are checked at every BNJson call site.

//! The `BNJson` document table and its C ABI. One table serves both backends:
//! `bn_lib_json` marshals `Value`s into these calls and the LLVM backend emits
//! them directly, so a handle means the same thing interpreted and compiled.
//!
//! Every operation fails with a [`JsonFailure`] (bnjson.md "Errors"). The
//! interpreter turns it into an `Error` value; the C ABI records it with the
//! member's name and returns a non-zero status, and the emitted code reads
//! the record back (`bn_rt_error_take`).
//!
//! Bounds are enforced where they are breached. A `set` that would push a
//! document past the depth limit fails at that call rather than leaving a
//! document that only `stringify` will later refuse.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::json::{MAX_DEPTH, parse, stringify};
use crate::json_error::{JsonFailure, Place, kind_of};
use crate::{c_str, c_string};

pub const BN_JSON_OK: i32 = 0;
/// A null pointer from the caller; emitted code never passes one.
pub const BN_JSON_INVALID_ARGUMENT: i32 = 1;
/// The handle was released.
pub const BN_JSON_INVALID_HANDLE: i32 = 2;
/// Any other failure; its report is recorded for `bn_rt_error_take`.
pub const BN_JSON_FAILED: i32 = 3;

type Outcome<T> = Result<T, JsonFailure>;

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

/// `Parse`: a new document from `text` under the 0.3 bounds.
pub fn parse_document(text: &str) -> Outcome<u64> {
    parse(text).map(store).map_err(JsonFailure::Parse)
}

/// `Stringify`: the document as JSON text.
pub fn stringify_document(handle: u64) -> Outcome<String> {
    let value = document(handle).ok_or(JsonFailure::InvalidHandle)?;
    stringify(&value).map_err(JsonFailure::Stringify)
}

/// A JSON number of `value`; JSON has no NaN or infinity.
pub fn number(value: f64) -> Outcome<serde_json::Value> {
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .ok_or(JsonFailure::NonFinite)
}

/// Writes a member of an object, failing closed on a non-object target and
/// on a write that would breach the depth bound.
pub fn set_value(handle: u64, key: &str, value: serde_json::Value) -> Outcome<()> {
    with_documents(|documents| {
        let target = documents
            .get_mut(&handle)
            .ok_or(JsonFailure::InvalidHandle)?;
        let found = kind_of(target);
        let object = target.as_object_mut().ok_or(JsonFailure::NotContainer {
            expected: "object",
            found,
        })?;
        let previous = object.insert(key.to_owned(), value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(object) = target.as_object_mut() {
                match previous {
                    Some(previous) => object.insert(key.to_owned(), previous),
                    None => object.remove(key),
                };
            }
            return Err(JsonFailure::Depth);
        }
        Ok(())
    })
}

/// Reads the member under `key` and projects it with `project`; `expected`
/// names the type for a member of another kind. Never a default value.
pub fn get_value<T>(
    handle: u64,
    key: &str,
    expected: &'static str,
    project: impl Fn(&serde_json::Value) -> Option<T>,
) -> Outcome<T> {
    with_documents(|documents| {
        let target = documents.get(&handle).ok_or(JsonFailure::InvalidHandle)?;
        let value = target
            .get(key)
            .ok_or_else(|| JsonFailure::Missing(key.to_owned()))?;
        project(value).ok_or_else(|| JsonFailure::WrongKind {
            place: Place::Key(key.to_owned()),
            expected,
            found: kind_of(value),
        })
    })
}

/// Reads a STRING member.
pub fn get_string(handle: u64, key: &str) -> Outcome<String> {
    get_value(handle, key, "STRING", |value| {
        value.as_str().map(str::to_owned)
    })
}

/// The kind of a document: `object`, `array`, `string`, `number`, `boolean` or
/// `null`. Queryable directly rather than guessed from `stringify` output.
#[must_use]
pub fn kind(handle: u64) -> Option<&'static str> {
    with_documents(|documents| documents.get(&handle).map(kind_of))
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

/// Member count for an object, element count for an array; a scalar has no
/// length.
pub fn length(handle: u64) -> Outcome<i64> {
    with_documents(|documents| {
        let value = documents.get(&handle).ok_or(JsonFailure::InvalidHandle)?;
        let count = match value {
            serde_json::Value::Object(entries) => entries.len(),
            serde_json::Value::Array(items) => items.len(),
            other => {
                return Err(JsonFailure::NotContainer {
                    expected: "object or array",
                    found: kind_of(other),
                });
            }
        };
        Ok(i64::try_from(count).unwrap_or(i64::MAX))
    })
}

/// Appends `value` to an array, failing closed on a non-array target and on an
/// append that would breach the depth bound.
pub fn append_value(handle: u64, value: serde_json::Value) -> Outcome<()> {
    with_documents(|documents| {
        let target = documents
            .get_mut(&handle)
            .ok_or(JsonFailure::InvalidHandle)?;
        let found = kind_of(target);
        let items = target.as_array_mut().ok_or(JsonFailure::NotContainer {
            expected: "array",
            found,
        })?;
        items.push(value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(items) = target.as_array_mut() {
                items.pop();
            }
            return Err(JsonFailure::Depth);
        }
        Ok(())
    })
}

/// The position of `index` in an array of `items`, or why there is none.
fn position(items: &[serde_json::Value], index: i64) -> Outcome<usize> {
    usize::try_from(index)
        .ok()
        .filter(|position| *position < items.len())
        .ok_or(JsonFailure::OutOfRange {
            index,
            length: items.len(),
        })
}

/// Reads element `index` and projects it; `expected` names the type for an
/// element of another kind.
pub fn element<T>(
    handle: u64,
    index: i64,
    expected: &'static str,
    project: impl Fn(&serde_json::Value) -> Option<T>,
) -> Outcome<T> {
    with_documents(|documents| {
        let target = documents.get(&handle).ok_or(JsonFailure::InvalidHandle)?;
        let items = target.as_array().ok_or(JsonFailure::NotContainer {
            expected: "array",
            found: kind_of(target),
        })?;
        let value = &items[position(items, index)?];
        project(value).ok_or(JsonFailure::WrongKind {
            place: Place::Index(index),
            expected,
            found: kind_of(value),
        })
    })
}

/// Writes element `index` of an array. The depth bound is checked at the
/// write, same as [`set_value`].
pub fn set_at(handle: u64, index: i64, value: serde_json::Value) -> Outcome<()> {
    with_documents(|documents| {
        let target = documents
            .get_mut(&handle)
            .ok_or(JsonFailure::InvalidHandle)?;
        let found = kind_of(target);
        let items = target.as_array_mut().ok_or(JsonFailure::NotContainer {
            expected: "array",
            found,
        })?;
        let slot = position(items, index)?;
        let previous = std::mem::replace(&mut items[slot], value);
        if depth_of(target) > MAX_DEPTH {
            if let Some(items) = target.as_array_mut() {
                items[slot] = previous;
            }
            return Err(JsonFailure::Depth);
        }
        Ok(())
    })
}

/// Duplicates a document into a new handle. Nesting moves rather than copies,
/// so duplication is always something the caller asked for by name.
#[must_use]
pub fn clone_document(handle: u64) -> Option<u64> {
    document(handle).map(store)
}

/// Moves `child` into a parent with `write`, consuming the child handle on
/// success: a value lives in exactly one place, so there is no aliasing, no
/// cycle can be built, and the depth check sees an inert child.
fn move_child(
    parent: u64,
    child: u64,
    write: impl FnOnce(serde_json::Value) -> Outcome<()>,
) -> Outcome<()> {
    if parent == child {
        return Err(JsonFailure::SelfMove);
    }
    let value = document(child).ok_or(JsonFailure::InvalidHandle)?;
    write(value)?;
    release(child);
    Ok(())
}

/// `SetJson`: moves `child` into `parent` under `key`.
pub fn move_into(parent: u64, key: &str, child: u64) -> Outcome<()> {
    move_child(parent, child, |value| set_value(parent, key, value))
}

/// `SetJsonAt`: moves `child` into `parent` at array `index`.
pub fn move_into_at(parent: u64, index: i64, child: u64) -> Outcome<()> {
    move_child(parent, child, |value| set_at(parent, index, value))
}

/// `AppendJson`: appends `child` onto an array.
pub fn append_moved(parent: u64, child: u64) -> Outcome<()> {
    move_child(parent, child, |value| append_value(parent, value))
}

/// A fresh handle to the member under `key`. The parent is not consumed; the
/// caller owns the new handle and must `RELEASE` it.
pub fn get_json(handle: u64, key: &str) -> Outcome<u64> {
    get_value(handle, key, "Json", |value| Some(value.clone())).map(store)
}

/// A fresh handle to array element `index`. The parent is not consumed.
pub fn get_json_at(handle: u64, index: i64) -> Outcome<u64> {
    element(handle, index, "Json", |value| Some(value.clone())).map(store)
}

// ---- C ABI ---------------------------------------------------------------

/// Records `failure` as the call's `Error` and returns its status.
fn failed(operation: &str, failure: &JsonFailure) -> i32 {
    crate::set_error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    );
    if *failure == JsonFailure::InvalidHandle {
        BN_JSON_INVALID_HANDLE
    } else {
        BN_JSON_FAILED
    }
}

/// The status of a VOID member.
fn status(operation: &str, outcome: Outcome<()>) -> i32 {
    outcome.map_or_else(|failure| failed(operation, &failure), |()| BN_JSON_OK)
}

/// A value member: its value, the status through `status`, and `fallback`
/// (never read by emitted code) on failure.
#[allow(unsafe_code)] // C ABI: writes the status through the caller's pointer.
fn answer<T>(operation: &str, status: *mut i32, outcome: Outcome<T>, fallback: T) -> T {
    if status.is_null() {
        return fallback;
    }
    let (code, value) = match outcome {
        Ok(value) => (BN_JSON_OK, value),
        Err(failure) => (failed(operation, &failure), fallback),
    };
    // SAFETY: `status` is non-null and points at the caller's i32 slot.
    unsafe { *status = code };
    value
}

/// A handle member: the handle through `out` and the status.
#[allow(unsafe_code)] // C ABI: writes the handle through the caller's pointer.
fn handle_out(operation: &str, out: *mut u64, outcome: Outcome<u64>) -> i32 {
    if out.is_null() {
        return BN_JSON_INVALID_ARGUMENT;
    }
    match outcome {
        Ok(handle) => {
            // SAFETY: `out` is non-null and points at the caller's u64 slot.
            unsafe { *out = handle };
            BN_JSON_OK
        }
        Err(failure) => failed(operation, &failure),
    }
}

/// A new, empty object. Returns its handle.
#[allow(unsafe_code)] // C ABI: no input, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_object() -> u64 {
    store(serde_json::Value::Object(serde_json::Map::new()))
}

/// A new, empty array.
#[allow(unsafe_code)] // C ABI: no input, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_array() -> u64 {
    store(serde_json::Value::Array(Vec::new()))
}

/// Parses `text` under the 0.3 bounds, writing the handle through `out`.
#[allow(unsafe_code)] // C ABI: STRING in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_parse(text: *const c_char, out: *mut u64) -> i32 {
    let Some(text) = c_str(text) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    handle_out("BNJson.Json.Parse", out, parse_document(text))
}

/// Serializes a document, writing the status through `status`. The returned
/// string is owned by the caller and is `""` on anything but [`BN_JSON_OK`].
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_stringify(handle: u64, status: *mut i32) -> *mut c_char {
    let text = answer(
        "BNJson.Json.Stringify",
        status,
        stringify_document(handle),
        String::new(),
    );
    c_string(&text)
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
    c_str(key).map_or(0, |key| i32::from(has(handle, key)))
}

/// Member or element count; `-1`, with the failure recorded, for a released
/// handle or a scalar.
#[allow(unsafe_code)] // C ABI: opaque handle in, count out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_length(handle: u64) -> i64 {
    length(handle).unwrap_or_else(|failure| {
        failed("BNJson.Json.Length", &failure);
        -1
    })
}

/// Duplicates a document, writing the new handle through `out`.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_clone(handle: u64, out: *mut u64) -> i32 {
    handle_out(
        "BNJson.Json.Clone",
        out,
        clone_document(handle).ok_or(JsonFailure::InvalidHandle),
    )
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

/// Defines a key write `$name(handle, key, args) -> status` of the value
/// `$value` (an `Outcome<serde_json::Value>` of the args).
macro_rules! key_write {
    ($name:ident, $member:literal, ($($arg:ident: $ty:ty),*), $value:expr) => {
        #[doc = concat!("`BNJson.Json.", $member, "`.")]
        #[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, status out.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(handle: u64, key: *const c_char $(, $arg: $ty)*) -> i32 {
            let Some(key) = c_str(key) else {
                return BN_JSON_INVALID_ARGUMENT;
            };
            let value: Outcome<serde_json::Value> = $value;
            status(
                concat!("BNJson.Json.", $member),
                value.and_then(|value| set_value(handle, key, value)),
            )
        }
    };
}

/// Defines an index write `$name(handle, index, args) -> status`.
macro_rules! index_write {
    ($name:ident, $member:literal, ($($arg:ident: $ty:ty),*), $value:expr) => {
        #[doc = concat!("`BNJson.Json.", $member, "`.")]
        #[allow(unsafe_code)] // C ABI: opaque handle in, status out.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(handle: u64, index: i64 $(, $arg: $ty)*) -> i32 {
            let value: Outcome<serde_json::Value> = $value;
            status(
                concat!("BNJson.Json.", $member),
                value.and_then(|value| set_at(handle, index, value)),
            )
        }
    };
}

/// Defines an append `$name(handle, args) -> status`.
macro_rules! append {
    ($name:ident, $member:literal, ($($arg:ident: $ty:ty),*), $value:expr) => {
        #[doc = concat!("`BNJson.Json.", $member, "`.")]
        #[allow(unsafe_code)] // C ABI: opaque handle in, status out.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(handle: u64 $(, $arg: $ty)*) -> i32 {
            let value: Outcome<serde_json::Value> = $value;
            status(
                concat!("BNJson.Json.", $member),
                value.and_then(|value| append_value(handle, value)),
            )
        }
    };
}

/// A STRING value; a null pointer never comes from emitted code.
#[allow(clippy::unnecessary_wraps)] // Same shape as `number` for the ABI macros.
fn text_value(value: *const c_char) -> Outcome<serde_json::Value> {
    Ok(serde_json::Value::String(
        c_str(value).unwrap_or_default().to_owned(),
    ))
}

#[allow(clippy::unnecessary_wraps)] // Same shape as `number` for the ABI macros.
fn integer_value(value: i64) -> Outcome<serde_json::Value> {
    Ok(serde_json::Value::from(value))
}

#[allow(clippy::unnecessary_wraps)] // Same shape as `number` for the ABI macros.
fn boolean_value(value: i32) -> Outcome<serde_json::Value> {
    Ok(serde_json::Value::Bool(value != 0))
}

const NULL_VALUE: Outcome<serde_json::Value> = Ok(serde_json::Value::Null);

key_write!(bn_rt_json_set_string, "SetString", (value: *const c_char), text_value(value));
key_write!(bn_rt_json_set_integer, "SetInteger", (value: i64), integer_value(value));
key_write!(bn_rt_json_set_float, "SetFloat", (value: f64), number(value));
key_write!(bn_rt_json_set_boolean, "SetBoolean", (value: i32), boolean_value(value));
key_write!(bn_rt_json_set_null, "SetNull", (), NULL_VALUE);

index_write!(bn_rt_json_set_string_at, "SetStringAt", (value: *const c_char), text_value(value));
index_write!(bn_rt_json_set_integer_at, "SetIntegerAt", (value: i64), integer_value(value));
index_write!(bn_rt_json_set_float_at, "SetFloatAt", (value: f64), number(value));
index_write!(bn_rt_json_set_boolean_at, "SetBooleanAt", (value: i32), boolean_value(value));
index_write!(bn_rt_json_set_null_at, "SetNullAt", (), NULL_VALUE);

append!(bn_rt_json_append_string, "AppendString", (value: *const c_char), text_value(value));
append!(bn_rt_json_append_integer, "AppendInteger", (value: i64), integer_value(value));
append!(bn_rt_json_append_float, "AppendFloat", (value: f64), number(value));
append!(bn_rt_json_append_boolean, "AppendBoolean", (value: i32), boolean_value(value));
append!(bn_rt_json_append_null, "AppendNull", (), NULL_VALUE);

/// Moves `child` into `parent` under `key`, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles plus STRING in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_json(parent: u64, key: *const c_char, child: u64) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    status("BNJson.Json.SetJson", move_into(parent, key, child))
}

/// Moves `child` into the array at `index`, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_set_json_at(parent: u64, index: i64, child: u64) -> i32 {
    status("BNJson.Json.SetJsonAt", move_into_at(parent, index, child))
}

/// Appends `child` onto an array, consuming the child handle.
#[allow(unsafe_code)] // C ABI: two opaque handles in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_append_json(parent: u64, child: u64) -> i32 {
    status("BNJson.Json.AppendJson", append_moved(parent, child))
}

/// Reads a STRING member into a freshly allocated string; `""` on failure,
/// which must not be read as a value.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_string(
    handle: u64,
    key: *const c_char,
    status: *mut i32,
) -> *mut c_char {
    let outcome = get_string(handle, c_str(key).unwrap_or_default());
    let text = answer("BNJson.Json.GetString", status, outcome, String::new());
    super::text_abi::reject_nul("BNJson.Json.GetString", text.as_bytes());
    c_string(&text)
}

/// Reads an INTEGER member, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_integer(handle: u64, key: *const c_char, status: *mut i32) -> i64 {
    let key = c_str(key).unwrap_or_default();
    let outcome = get_value(handle, key, "INTEGER", serde_json::Value::as_i64);
    answer("BNJson.Json.GetInteger", status, outcome, 0)
}

/// Reads a FLOAT member, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_float(handle: u64, key: *const c_char, status: *mut i32) -> f64 {
    let key = c_str(key).unwrap_or_default();
    let outcome = get_value(handle, key, "FLOAT", serde_json::Value::as_f64);
    answer("BNJson.Json.GetFloat", status, outcome, 0.0)
}

/// Reads a BOOLEAN member as `0` or `1`, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_boolean(handle: u64, key: *const c_char, status: *mut i32) -> i32 {
    let key = c_str(key).unwrap_or_default();
    let outcome = get_value(handle, key, "BOOLEAN", serde_json::Value::as_bool);
    i32::from(answer("BNJson.Json.GetBoolean", status, outcome, false))
}

/// A fresh handle to the member under `key`, writing it through `out`. The
/// parent is not consumed; the caller owns the new handle.
#[allow(unsafe_code)] // C ABI: opaque handle plus STRING in, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_json(handle: u64, key: *const c_char, out: *mut u64) -> i32 {
    let Some(key) = c_str(key) else {
        return BN_JSON_INVALID_ARGUMENT;
    };
    handle_out("BNJson.Json.GetJson", out, get_json(handle, key))
}

/// Reads a STRING element; `""` on failure.
#[allow(unsafe_code)] // C ABI: opaque handle in, owned STRING out plus status.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_string_at(
    handle: u64,
    index: i64,
    status: *mut i32,
) -> *mut c_char {
    let outcome = element(handle, index, "STRING", |value| {
        value.as_str().map(str::to_owned)
    });
    let text = answer("BNJson.Json.GetStringAt", status, outcome, String::new());
    super::text_abi::reject_nul("BNJson.Json.GetStringAt", text.as_bytes());
    c_string(&text)
}

/// Reads an INTEGER element, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_integer_at(handle: u64, index: i64, status: *mut i32) -> i64 {
    let outcome = element(handle, index, "INTEGER", serde_json::Value::as_i64);
    answer("BNJson.Json.GetIntegerAt", status, outcome, 0)
}

/// Reads a FLOAT element, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_float_at(handle: u64, index: i64, status: *mut i32) -> f64 {
    let outcome = element(handle, index, "FLOAT", serde_json::Value::as_f64);
    answer("BNJson.Json.GetFloatAt", status, outcome, 0.0)
}

/// Reads a BOOLEAN element as `0` or `1`, writing the status through `status`.
#[allow(unsafe_code)] // C ABI: opaque handle in, value plus status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_boolean_at(handle: u64, index: i64, status: *mut i32) -> i32 {
    let outcome = element(handle, index, "BOOLEAN", serde_json::Value::as_bool);
    i32::from(answer("BNJson.Json.GetBooleanAt", status, outcome, false))
}

/// A fresh handle to element `index`, writing it through `out`.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_get_json_at(handle: u64, index: i64, out: *mut u64) -> i32 {
    handle_out("BNJson.Json.GetJsonAt", out, get_json_at(handle, index))
}

#[cfg(test)]
#[path = "json_abi_tests.rs"]
mod tests;
