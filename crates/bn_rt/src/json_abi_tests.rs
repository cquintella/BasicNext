// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::*;
use crate::json_error::{JsonFailure, Place};
use std::ffi::CString;

#[test]
fn object_round_trips_and_fails_closed() {
    let handle = bn_rt_json_object();
    assert_eq!(
        set_value(handle, "name", serde_json::Value::from("pardal")),
        Ok(())
    );
    assert_eq!(get_string(handle, "name").as_deref(), Ok("pardal"));
    // A missing key is an error, never an empty string.
    assert_eq!(
        get_string(handle, "absent"),
        Err(JsonFailure::Missing("absent".into()))
    );
    assert!(release(handle));
    // Released once, gone for good.
    assert!(!release(handle));
    assert_eq!(get_string(handle, "name"), Err(JsonFailure::InvalidHandle));
}

#[test]
fn nesting_moves_the_child_and_cannot_build_a_cycle() {
    let parent = bn_rt_json_object();
    let child = bn_rt_json_object();
    assert_eq!(set_value(child, "k", serde_json::Value::from("v")), Ok(()));
    assert_eq!(move_into(parent, "nest", child), Ok(()));
    // The child handle is consumed: a value lives in exactly one place.
    assert!(document(child).is_none());
    assert_eq!(
        move_into(parent, "again", child),
        Err(JsonFailure::InvalidHandle)
    );
    let copy = get_json(parent, "nest").expect("nested clone");
    assert_eq!(get_string(copy, "k").as_deref(), Ok("v"));
    assert!(document(parent).is_some());
    // The shortest possible cycle is refused outright.
    let solo = bn_rt_json_object();
    assert_eq!(move_into(solo, "self", solo), Err(JsonFailure::SelfMove));
    // Clone is the explicit way to duplicate, and the copy is independent.
    let original = bn_rt_json_array();
    let duplicate = clone_document(original).expect("clone");
    assert_ne!(original, duplicate);
    assert!(release(original));
    assert!(document(duplicate).is_some());
}

#[test]
fn reads_tell_missing_from_wrong_kind_and_out_of_range() {
    let object = parse_document(r#"{"n":1,"items":["a"]}"#).expect("parse");
    assert_eq!(
        get_string(object, "n"),
        Err(JsonFailure::WrongKind {
            place: Place::Key("n".into()),
            expected: "STRING",
            found: "number",
        })
    );
    let items = get_json(object, "items").expect("items");
    assert_eq!(
        element(items, 3, "STRING", |value| value
            .as_str()
            .map(str::to_owned)),
        Err(JsonFailure::OutOfRange {
            index: 3,
            length: 1
        })
    );
    assert_eq!(
        set_at(items, -1, serde_json::Value::Null).map_err(|f| f.code()),
        Err(4)
    );
    assert_eq!(
        append_value(object, serde_json::Value::Null),
        Err(JsonFailure::NotContainer {
            expected: "array",
            found: "object"
        })
    );
    assert!(matches!(
        length(get_json(object, "n").expect("scalar")),
        Err(JsonFailure::NotContainer {
            found: "number",
            ..
        })
    ));
    assert!(matches!(
        parse_document("{oops"),
        Err(JsonFailure::Parse(_))
    ));
}

#[test]
fn abi_records_the_failure_for_the_emitted_code() {
    let handle = bn_rt_json_object();
    let key = CString::new("pi").unwrap();
    assert_ne!(
        bn_rt_json_set_float(handle, key.as_ptr(), f64::NAN),
        BN_JSON_OK
    );
    let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
    assert_eq!(
        crate::error_abi::bn_rt_error_code(record),
        i64::from(bn_types::error_codes::json::INVALID_ARGUMENT)
    );
    assert_eq!(bn_rt_json_set_float(handle, key.as_ptr(), 3.5), BN_JSON_OK);
    let mut status = -1;
    let value = bn_rt_json_get_float(handle, key.as_ptr(), &raw mut status);
    assert_eq!(status, BN_JSON_OK);
    assert!((value - 3.5).abs() < f64::EPSILON);
    let absent = CString::new("absent").unwrap();
    let _ = bn_rt_json_get_integer(handle, absent.as_ptr(), &raw mut status);
    assert_eq!(status, BN_JSON_FAILED);
    let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
    assert_eq!(
        crate::error_abi::bn_rt_error_code(record),
        i64::from(bn_types::error_codes::json::NOT_FOUND)
    );
    assert!(release(handle));
}
