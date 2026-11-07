// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(unsafe_code, clippy::similar_names)]

use bn_stdlib::crypto::constant_time_equals;
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_constant_time_equals, bn_rt_crypto_bytes_from_text,
    bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release, bn_rt_crypto_bytes_to_hex,
    bn_rt_crypto_secret_constant_time_equals, bn_rt_crypto_secret_from_text,
    bn_rt_crypto_secret_length, bn_rt_crypto_secret_release, bn_rt_crypto_secret_to_bytes,
    bn_rt_crypto_secret_zeroize,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use std::ffi::c_char;

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

#[test]
fn test_constant_time_equals_unit() {
    assert!(constant_time_equals(b"secret123", b"secret123"));
    assert!(!constant_time_equals(b"secret123", b"secret124"));
    assert!(!constant_time_equals(b"secret123", b"secret12"));
    assert!(!constant_time_equals(b"", b"a"));
    assert!(constant_time_equals(b"", b""));
}

#[test]
fn test_bytes_constant_time_equals_abi() {
    let t1 = BnString::new(b"symmetric-key-material-32-bytes!");
    let t2 = BnString::new(b"symmetric-key-material-32-bytes!");
    let t3 = BnString::new(b"symmetric-key-material-32-bytes?");

    let h1 = bn_rt_crypto_bytes_from_text(t1.as_ptr());
    let h2 = bn_rt_crypto_bytes_from_text(t2.as_ptr());
    let h3 = bn_rt_crypto_bytes_from_text(t3.as_ptr());

    assert_eq!(bn_rt_crypto_bytes_constant_time_equals(h1, h2), 1);
    assert_eq!(bn_rt_crypto_bytes_constant_time_equals(h1, h3), 0);
    assert_eq!(bn_rt_crypto_bytes_constant_time_equals(h1, 999_999), 0);

    bn_rt_crypto_bytes_release(h1);
    bn_rt_crypto_bytes_release(h2);
    bn_rt_crypto_bytes_release(h3);
}

#[test]
fn test_secret_lifecycle_and_operations_abi() {
    let t1 = BnString::new(b"my-api-token-value");
    let t2 = BnString::new(b"my-api-token-value");
    let t3 = BnString::new(b"other-token-value");

    let s1 = bn_rt_crypto_secret_from_text(t1.as_ptr());
    let s2 = bn_rt_crypto_secret_from_text(t2.as_ptr());
    let s3 = bn_rt_crypto_secret_from_text(t3.as_ptr());

    assert_ne!(s1, 0);
    assert_ne!(s2, 0);
    assert_ne!(s3, 0);

    assert_eq!(bn_rt_crypto_secret_length(s1), 18);
    assert_eq!(bn_rt_crypto_secret_constant_time_equals(s1, s2), 1);
    assert_eq!(bn_rt_crypto_secret_constant_time_equals(s1, s3), 0);

    // Convert secret to Bytes
    let b1 = bn_rt_crypto_secret_to_bytes(s1);
    assert_ne!(b1, 0);
    assert_eq!(bn_rt_crypto_bytes_length(b1), 18);

    // Zeroize secret
    assert_eq!(bn_rt_crypto_secret_zeroize(s1), 0);
    assert_eq!(bn_rt_crypto_secret_constant_time_equals(s1, s2), 0);

    let b1_zero = bn_rt_crypto_secret_to_bytes(s1);
    let hex_ptr = bn_rt_crypto_bytes_to_hex(b1_zero);
    assert!(!hex_ptr.is_null());
    // SAFETY: the runtime returned this NUL-terminated STRING, alive until it is released.
    let hex_str = unsafe { std::ffi::CStr::from_ptr(hex_ptr) }
        .to_str()
        .unwrap();
    assert_eq!(hex_str, "00".repeat(18));
    bn_rt_str_release(hex_ptr);

    bn_rt_crypto_bytes_release(b1);
    bn_rt_crypto_bytes_release(b1_zero);
    assert_eq!(bn_rt_crypto_secret_release(s1), 0);
    assert_eq!(bn_rt_crypto_secret_release(s2), 0);
    assert_eq!(bn_rt_crypto_secret_release(s3), 0);
}
