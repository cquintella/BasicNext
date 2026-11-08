// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(unsafe_code)]
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_stdlib::crypto::{bn_decrypt_envelope, bn_encrypt_envelope};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_text, bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release,
    bn_rt_crypto_secret_key_generate, bn_rt_crypto_secret_key_release,
};
use bn_stdlib::crypto_envelope::{
    bn_rt_crypto_decrypt_status, bn_rt_crypto_decrypt_text_status, bn_rt_crypto_encrypt_status,
    bn_rt_crypto_encrypt_text_status,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes, slice_from_raw};
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
fn test_envelope_roundtrip_and_tamper() {
    let key = [0x42u8; 32];
    let plaintext = b"Hello, Basic Next authenticated encryption envelopes!";

    let sealed = bn_encrypt_envelope(&key, plaintext).expect("encrypt envelope");
    assert!(
        sealed.starts_with(b"BNC1"),
        "sealed data must start with BNC1 magic"
    );
    // Header is 31 bytes (4 magic + 1 ver + 1 alg + 1 flags + 24 nonce) + plaintext + 16 tag
    assert_eq!(sealed.len(), 31 + plaintext.len() + 16);

    let decrypted = bn_decrypt_envelope(&key, &sealed).expect("decrypt envelope");
    assert_eq!(decrypted, plaintext);

    // Tamper header
    let mut tampered_header = sealed.clone();
    tampered_header[0] = b'X';
    assert!(bn_decrypt_envelope(&key, &tampered_header).is_err());

    // Tamper ciphertext
    let mut tampered_ct = sealed.clone();
    let ct_offset = 31 + 5;
    tampered_ct[ct_offset] ^= 0x01;
    assert!(bn_decrypt_envelope(&key, &tampered_ct).is_err());

    // Tamper tag
    let mut tampered_tag = sealed.clone();
    let last = tampered_tag.len() - 1;
    tampered_tag[last] ^= 0x01;
    assert!(bn_decrypt_envelope(&key, &tampered_tag).is_err());

    // Wrong key
    let wrong_key = [0x43u8; 32];
    assert!(bn_decrypt_envelope(&wrong_key, &sealed).is_err());
}

#[test]
fn test_envelope_c_abi() {
    let key_handle = bn_rt_crypto_secret_key_generate(256, std::ptr::null(), std::ptr::null());
    assert!(key_handle > 0);

    let text = BnString::new(b"Sensitive payload data");
    let text_handle = bn_rt_crypto_bytes_from_text(text.as_ptr());
    assert!(text_handle > 0);

    let mut sealed: i64 = 0;
    assert_eq!(
        bn_rt_crypto_encrypt_status(key_handle, text_handle, &raw mut sealed),
        0
    );
    let sealed_handle = u64::try_from(sealed).expect("handle");
    assert!(sealed_handle > 0);
    let sealed_len = bn_rt_crypto_bytes_length(sealed_handle);
    assert_eq!(sealed_len, 31 + 22 + 16);

    let mut decrypted: i64 = 0;
    assert_eq!(
        bn_rt_crypto_decrypt_status(key_handle, sealed_handle, &raw mut decrypted),
        0
    );
    let decrypted_handle = u64::try_from(decrypted).expect("handle");
    assert_eq!(bn_rt_crypto_bytes_length(decrypted_handle), 22);

    // A wrong key is a failure the status reports, never a handle.
    let other_key = bn_rt_crypto_secret_key_generate(256, std::ptr::null(), std::ptr::null());
    let mut refused: i64 = 0;
    assert_ne!(
        bn_rt_crypto_decrypt_status(other_key, sealed_handle, &raw mut refused),
        0
    );
    assert_eq!(refused, 0);

    // Text helper C ABI
    let mut token_ptr: *mut c_char = std::ptr::null_mut();
    assert_eq!(
        bn_rt_crypto_encrypt_text_status(key_handle, text.as_ptr(), &raw mut token_ptr),
        0
    );
    assert!(!token_ptr.is_null());

    let mut recovered_ptr: *mut c_char = std::ptr::null_mut();
    assert_eq!(
        bn_rt_crypto_decrypt_text_status(key_handle, token_ptr, &raw mut recovered_ptr),
        0
    );
    let recovered_bytes = slice_from_raw(recovered_ptr).expect("slice from raw STRING");
    let unwrapped_text = std::str::from_utf8(recovered_bytes).unwrap();
    assert_eq!(unwrapped_text, "Sensitive payload data");

    // Clean up
    bn_rt_str_release(token_ptr);
    bn_rt_str_release(recovered_ptr);
    bn_rt_crypto_bytes_release(text_handle);
    bn_rt_crypto_bytes_release(sealed_handle);
    bn_rt_crypto_bytes_release(decrypted_handle);
    bn_rt_crypto_secret_key_release(key_handle);
    bn_rt_crypto_secret_key_release(other_key);
}
