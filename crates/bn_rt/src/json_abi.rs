// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // Every failure is a `JsonFailure`.
#![allow(clippy::doc_markdown)] // Status constant names appear bare in prose.
#![allow(clippy::must_use_candidate)] // Statuses are checked at every BNJson call site.

//! C ABI for compiled `BNJson`. The document table lives in `bn_core_json`;
//! these exports only marshal pointers and record `JsonFailure` for
//! `bn_rt_error_take`.

use std::ffi::c_char;

use bn_core_json::{
    JsonFailure, append_moved, append_value, array, clone_document, element, get_json, get_json_at,
    get_string, get_value, has, kind, length, move_into, move_into_at, number, object,
    parse_document, release, set_at, set_value, stringify_document,
};

use crate::{c_str, c_string};

pub const BN_JSON_OK: i32 = 0;
/// A null pointer from the caller; emitted code never passes one.
pub const BN_JSON_INVALID_ARGUMENT: i32 = 1;
/// The handle was released.
pub const BN_JSON_INVALID_HANDLE: i32 = 2;
/// Any other failure; its report is recorded for `bn_rt_error_take`.
pub const BN_JSON_FAILED: i32 = 3;

type Outcome<T> = Result<T, JsonFailure>;

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
    object()
}

/// A new, empty array.
#[allow(unsafe_code)] // C ABI: no input, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_json_array() -> u64 {
    array()
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
