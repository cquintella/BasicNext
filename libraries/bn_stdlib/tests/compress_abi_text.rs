// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A text argument that reaches the `BNCompress` C ABI null or not valid UTF-8
//! is `INVALID_ARGUMENT` naming the operation, never an empty path or name
//! (`ongoing/issues.md`, A17).

use std::ffi::c_char;

use bn_core_compress::{CODE_INVALID_ARGUMENT, CODE_NOT_FOUND};
use bn_stdlib::compress_abi::{
    bn_rt_compress_deflate_text, bn_rt_zip_add_text, bn_rt_zip_create, bn_rt_zip_extract_file,
    bn_rt_zip_open,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use bn_stdlib::take_error_report;

/// A handle the table never issued.
const UNKNOWN_HANDLE: u64 = u64::MAX;

/// A heap BN `STRING` (header + payload), released on drop.
struct BnString(*mut c_char);

impl BnString {
    fn new(bytes: &[u8]) -> Self {
        let pointer = c_string_from_bytes(bytes);
        assert!(!pointer.is_null(), "allocate STRING");
        Self(pointer)
    }

    fn as_ptr(&self) -> *const c_char {
        self.0
    }
}

impl Drop for BnString {
    fn drop(&mut self) {
        bn_rt_str_release(self.0);
    }
}

fn assert_invalid_text(code: i32, operation: &str) {
    assert_eq!(code, CODE_INVALID_ARGUMENT, "{operation}");
    let (reported, reported_operation, message, _) = take_error_report();
    assert_eq!(reported, CODE_INVALID_ARGUMENT, "{operation}");
    assert_eq!(reported_operation, operation);
    assert!(!message.is_empty(), "{operation}");
}

#[test]
fn null_and_non_utf8_paths_are_invalid_arguments() {
    let bad = BnString::new(&[0xff, 0xfe]);
    let mut handle = 0u64;
    for path in [std::ptr::null(), bad.as_ptr()] {
        assert_invalid_text(bn_rt_zip_create(path, &raw mut handle), "ZipArchive.Create");
        assert_invalid_text(bn_rt_zip_open(path, &raw mut handle), "ZipArchive.Open");
    }
    assert_eq!(handle, 0, "no archive is stored for a rejected path");
}

#[test]
fn every_text_argument_is_checked_before_the_handle() {
    let bad = BnString::new(&[0xff, 0xfe]);
    let good = BnString::new(b"entry.txt");
    let null: *const c_char = std::ptr::null();
    assert_invalid_text(
        bn_rt_zip_add_text(UNKNOWN_HANDLE, null, good.as_ptr()),
        "ZipArchive.AddText",
    );
    assert_invalid_text(
        bn_rt_zip_add_text(UNKNOWN_HANDLE, good.as_ptr(), bad.as_ptr()),
        "ZipArchive.AddText",
    );
    assert_invalid_text(
        bn_rt_zip_extract_file(UNKNOWN_HANDLE, good.as_ptr(), null),
        "ZipArchive.ExtractFile",
    );
    let mut buffer = 0u64;
    assert_invalid_text(
        bn_rt_compress_deflate_text(bad.as_ptr(), &raw mut buffer),
        "DeflateText",
    );
    // Valid text passes the check and reaches the handle lookup.
    assert_eq!(
        bn_rt_zip_add_text(UNKNOWN_HANDLE, good.as_ptr(), good.as_ptr()),
        CODE_NOT_FOUND
    );
}
