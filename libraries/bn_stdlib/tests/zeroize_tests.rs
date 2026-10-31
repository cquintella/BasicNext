// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for memory zeroization and safe crypto buffer lifecycle (MEM-1).
#![allow(unsafe_code)]

use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_text, bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release,
    bn_rt_crypto_bytes_to_hex, bn_rt_crypto_bytes_zeroize, bn_rt_crypto_generate_key,
    bn_rt_crypto_generate_nonce, bn_rt_crypto_key_zeroize, bn_rt_crypto_open, bn_rt_crypto_seal,
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
fn test_bytes_zeroize_overwrites_memory_with_zeros() {
    let text = BnString::new(b"super-secret-password-123");
    let handle = bn_rt_crypto_bytes_from_text(text.as_ptr());
    assert_ne!(handle, 0);

    let len = bn_rt_crypto_bytes_length(handle);
    assert_eq!(len, 25);

    // Call zeroize
    let status = bn_rt_crypto_bytes_zeroize(handle);
    assert_eq!(status, 0);

    // Length is still 25, but content in hex is all zeros
    let len_after = bn_rt_crypto_bytes_length(handle);
    assert_eq!(len_after, 25);

    let hex_ptr = bn_rt_crypto_bytes_to_hex(handle);
    // SAFETY: hex_ptr points to valid null-terminated C string returned by runtime.
    let hex_str = unsafe { std::ffi::CStr::from_ptr(hex_ptr) }
        .to_str()
        .unwrap();
    assert_eq!(hex_str, "00".repeat(25));
    bn_rt_str_release(hex_ptr);

    // Release buffer
    assert_eq!(bn_rt_crypto_bytes_release(handle), 0);
    // After release, zeroize or length returns invalid handle
    assert_ne!(bn_rt_crypto_bytes_zeroize(handle), 0);
    assert_eq!(bn_rt_crypto_bytes_length(handle), -1);
}

#[test]
fn test_temporary_buffers_in_seal_and_open() {
    let mut key_handle = 0u64;
    assert_eq!(bn_rt_crypto_generate_key(1, &mut key_handle), 0); // AES-GCM
    assert_ne!(key_handle, 0);
    let mut nonce_handle = 0u64;
    assert_eq!(bn_rt_crypto_generate_nonce(1, &mut nonce_handle), 0);
    assert_ne!(nonce_handle, 0);

    let text = BnString::new(b"confidential-message");
    let pt_handle = bn_rt_crypto_bytes_from_text(text.as_ptr());
    let aad_text = BnString::new(b"public-aad");
    let aad_handle = bn_rt_crypto_bytes_from_text(aad_text.as_ptr());

    let mut ct_handle = 0u64;
    let seal_res = bn_rt_crypto_seal(
        0,
        key_handle,
        nonce_handle,
        pt_handle,
        aad_handle,
        &mut ct_handle,
    );
    assert_eq!(seal_res, 0);
    assert_ne!(ct_handle, 0);

    let mut recovered_handle = 0u64;
    let open_res = bn_rt_crypto_open(
        0,
        key_handle,
        nonce_handle,
        ct_handle,
        aad_handle,
        &mut recovered_handle,
    );
    assert_eq!(open_res, 0);
    assert_ne!(recovered_handle, 0);

    let hex_ptr = bn_rt_crypto_bytes_to_hex(recovered_handle);
    // SAFETY: hex_ptr points to valid null-terminated C string returned by runtime.
    let hex_str = unsafe { std::ffi::CStr::from_ptr(hex_ptr) }
        .to_str()
        .unwrap();
    assert_eq!(hex_str, "636f6e666964656e7469616c2d6d657373616765");
    bn_rt_str_release(hex_ptr);

    // Clean up
    bn_rt_crypto_bytes_release(pt_handle);
    bn_rt_crypto_bytes_release(aad_handle);
    bn_rt_crypto_bytes_release(nonce_handle);
    bn_rt_crypto_bytes_release(ct_handle);
    bn_rt_crypto_bytes_release(recovered_handle);
    bn_rt_crypto_key_zeroize(key_handle);
}
