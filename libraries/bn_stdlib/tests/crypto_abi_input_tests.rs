// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The crypto ABI refuses an input that is not a BN STRING instead of storing
// an empty buffer or secret: an empty key or MAC key must never come from a
// boundary failure. A BN program cannot reach this (every STRING is valid
// UTF-8 and emitted code never passes null); FFI or an emitter defect could.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_hex, bn_rt_crypto_bytes_from_text, bn_rt_crypto_bytes_length,
};
use bn_stdlib::crypto_secret_abi::{bn_rt_crypto_secret_from_text, bn_rt_crypto_secret_length};
use bn_types::error_codes::crypto::INVALID_ARGUMENT;

#[test]
fn a_null_text_makes_no_buffer() {
    let handle = bn_rt_crypto_bytes_from_text(std::ptr::null());
    assert_eq!(handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), -1);
}

#[test]
fn a_null_text_makes_no_secret() {
    let handle = bn_rt_crypto_secret_from_text(std::ptr::null());
    assert_eq!(handle, 0);
    assert_eq!(bn_rt_crypto_secret_length(handle), -1);
}

#[test]
fn a_null_hex_text_is_invalid_argument() {
    let mut handle = 0_u64;
    assert_eq!(
        bn_rt_crypto_bytes_from_hex(std::ptr::null(), &raw mut handle),
        INVALID_ARGUMENT
    );
    assert_eq!(handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), -1);
}

#[test]
fn a_non_bn_pointer_makes_no_buffer_or_secret() {
    let raw = b"not a bn string\0";
    let ptr = raw.as_ptr().cast::<std::ffi::c_char>();
    let handle = bn_rt_crypto_bytes_from_text(ptr);
    assert_eq!(handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), -1);

    let secret_handle = bn_rt_crypto_secret_from_text(ptr);
    assert_eq!(secret_handle, 0);
    assert_eq!(bn_rt_crypto_secret_length(secret_handle), -1);

    let mut hex_handle = 0_u64;
    assert_eq!(
        bn_rt_crypto_bytes_from_hex(ptr, &raw mut hex_handle),
        INVALID_ARGUMENT
    );
    assert_eq!(hex_handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(hex_handle), -1);
}

#[test]
fn an_invalid_utf8_text_makes_no_buffer_or_secret() {
    let bad_utf8 = bn_stdlib::string::c_string_from_bytes(b"\xff\xfe\xfd");
    let handle = bn_rt_crypto_bytes_from_text(bad_utf8);
    assert_eq!(handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), -1);

    let secret_handle = bn_rt_crypto_secret_from_text(bad_utf8);
    assert_eq!(secret_handle, 0);
    assert_eq!(bn_rt_crypto_secret_length(secret_handle), -1);

    let mut hex_handle = 0_u64;
    assert_eq!(
        bn_rt_crypto_bytes_from_hex(bad_utf8, &raw mut hex_handle),
        INVALID_ARGUMENT
    );
    assert_eq!(hex_handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(hex_handle), -1);

    bn_stdlib::string::bn_rt_str_release(bad_utf8);
}

#[test]
fn a_valid_text_still_makes_a_buffer() {
    let text = bn_stdlib::string::c_string_from_bytes(b"abc");
    let handle = bn_rt_crypto_bytes_from_text(text);
    bn_stdlib::string::bn_rt_str_release(text);
    assert_ne!(handle, 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), 3);
}
