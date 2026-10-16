// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // Every failure is a `JsonFailure`.
#![allow(clippy::must_use_candidate)] // Callers check the handle or the bool.

//! The `BNJson` document table. One table serves both adapters: `bn_lib_json`
//! calls this Rust API and `bn_rt::json_abi` wraps the same functions, so a
//! handle means the same thing interpreted and compiled (W3).
//!
//! Bounds are enforced where they are breached. A `set` that would push a
//! document past the depth limit fails at that call rather than leaving a
//! document that only `stringify` will later refuse.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::error::{JsonFailure, Place, kind_of};
use crate::parse::{MAX_DEPTH, parse, stringify};

type Outcome<T> = Result<T, JsonFailure>;

/// The documents of the module copy that runs on this thread
/// (`bn_core_copy`): a task's are freed when the task ends.
#[derive(Default)]
struct Documents(Mutex<HashMap<u64, serde_json::Value>>);

fn next_handle() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn with_documents<T>(operation: impl FnOnce(&mut HashMap<u64, serde_json::Value>) -> T) -> T {
    let documents = bn_core_copy::table::<Documents>();
    operation(&mut documents.0.lock().unwrap_or_else(PoisonError::into_inner))
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

/// A new, empty object. Returns its handle.
#[must_use]
pub fn object() -> u64 {
    store(serde_json::Value::Object(serde_json::Map::new()))
}

/// A new, empty array.
#[must_use]
pub fn array() -> u64 {
    store(serde_json::Value::Array(Vec::new()))
}
