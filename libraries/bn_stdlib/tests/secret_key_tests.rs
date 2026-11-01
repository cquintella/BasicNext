// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::similar_names,
    clippy::borrow_as_ptr
)]

use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_hex, bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release,
    bn_rt_crypto_bytes_to_hex,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use std::ffi::{CStr, c_char};

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

unsafe extern "C" {
    fn bn_rt_crypto_secret_key_generate(
        bits: i64,
        purpose: *const c_char,
        alg: *const c_char,
    ) -> u64;
    fn bn_rt_crypto_secret_key_from_bytes(
        bytes: u64,
        purpose: *const c_char,
        alg: *const c_char,
    ) -> u64;
    fn bn_rt_crypto_secret_key_id(key: u64) -> *mut c_char;
    fn bn_rt_crypto_secret_key_bits(key: u64) -> i64;
    fn bn_rt_crypto_secret_key_length(key: u64) -> i64;
    fn bn_rt_crypto_secret_key_purpose(key: u64) -> *mut c_char;
    fn bn_rt_crypto_secret_key_algorithm(key: u64) -> *mut c_char;
    fn bn_rt_crypto_secret_key_export(key: u64, format: *const c_char) -> u64;
    fn bn_rt_crypto_secret_key_wrap(kek: u64, key: u64) -> u64;
    fn bn_rt_crypto_secret_key_unwrap(
        kek: u64,
        wrapped: u64,
        bits: i64,
        purpose: *const c_char,
        alg: *const c_char,
    ) -> u64;
    fn bn_rt_crypto_secret_key_zeroize(key: u64) -> i32;
    fn bn_rt_crypto_secret_key_retain(key: u64) -> i32;
    fn bn_rt_crypto_secret_key_release(key: u64) -> i32;
}

#[test]
fn test_secret_key_lifecycle_and_wrap_abi() {
    let purpose = BnString::new(b"AEAD");
    let alg = BnString::new(b"AES256");

    let k1 = unsafe { bn_rt_crypto_secret_key_generate(256, purpose.as_ptr(), alg.as_ptr()) };
    assert_ne!(k1, 0, "secret key handle valid");
    assert_eq!(unsafe { bn_rt_crypto_secret_key_bits(k1) }, 256);
    assert_eq!(unsafe { bn_rt_crypto_secret_key_length(k1) }, 32);

    let id_ptr = unsafe { bn_rt_crypto_secret_key_id(k1) };
    assert!(!id_ptr.is_null());
    let id_str = unsafe { CStr::from_ptr(id_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(id_str.len(), 64);
    bn_rt_str_release(id_ptr);

    let purpose_ptr = unsafe { bn_rt_crypto_secret_key_purpose(k1) };
    assert_eq!(
        unsafe { CStr::from_ptr(purpose_ptr) }.to_str().unwrap(),
        "AEAD"
    );
    bn_rt_str_release(purpose_ptr);

    let alg_ptr = unsafe { bn_rt_crypto_secret_key_algorithm(k1) };
    assert_eq!(
        unsafe { CStr::from_ptr(alg_ptr) }.to_str().unwrap(),
        "AES256"
    );
    bn_rt_str_release(alg_ptr);

    assert_eq!(unsafe { bn_rt_crypto_secret_key_retain(k1) }, 0);
    assert_eq!(unsafe { bn_rt_crypto_secret_key_release(k1) }, 0);

    let format = BnString::new(b"RAW");
    let raw_bytes = unsafe { bn_rt_crypto_secret_key_export(k1, format.as_ptr()) };
    assert_ne!(raw_bytes, 0);
    assert_eq!(bn_rt_crypto_bytes_length(raw_bytes), 32);

    // Reconstruct key from raw bytes
    let k1_imported =
        unsafe { bn_rt_crypto_secret_key_from_bytes(raw_bytes, purpose.as_ptr(), alg.as_ptr()) };
    assert_ne!(k1_imported, 0);
    let id2_ptr = unsafe { bn_rt_crypto_secret_key_id(k1_imported) };
    assert_eq!(unsafe { CStr::from_ptr(id2_ptr) }.to_str().unwrap(), id_str);
    bn_rt_str_release(id2_ptr);

    // KEK generation and wrapping
    let kek = unsafe { bn_rt_crypto_secret_key_generate(256, purpose.as_ptr(), alg.as_ptr()) };
    assert_ne!(kek, 0);

    let wrapped = unsafe { bn_rt_crypto_secret_key_wrap(kek, k1) };
    assert_ne!(wrapped, 0, "wrapped bytes valid");
    assert_eq!(bn_rt_crypto_bytes_length(wrapped), 40);

    let unwrapped = unsafe {
        bn_rt_crypto_secret_key_unwrap(kek, wrapped, 256, purpose.as_ptr(), alg.as_ptr())
    };
    assert_ne!(unwrapped, 0, "unwrapped handle valid");
    let id_unwrapped_ptr = unsafe { bn_rt_crypto_secret_key_id(unwrapped) };
    assert_eq!(
        unsafe { CStr::from_ptr(id_unwrapped_ptr) }
            .to_str()
            .unwrap(),
        id_str
    );
    bn_rt_str_release(id_unwrapped_ptr);

    // Zeroize key
    assert_eq!(unsafe { bn_rt_crypto_secret_key_zeroize(k1) }, 0);
    assert_eq!(unsafe { bn_rt_crypto_secret_key_bits(k1) }, 0);

    bn_rt_crypto_bytes_release(raw_bytes);
    bn_rt_crypto_bytes_release(wrapped);
    unsafe {
        bn_rt_crypto_secret_key_release(k1);
        bn_rt_crypto_secret_key_release(k1_imported);
        bn_rt_crypto_secret_key_release(kek);
        bn_rt_crypto_secret_key_release(unwrapped);
    }
}

#[test]
fn test_rfc3394_vectors() {
    // RFC 3394 Section 4.1: Wrap 128 bits of Key Data with a 128-bit KEK
    let hex_kek = BnString::new(b"000102030405060708090A0B0C0D0E0F");
    let hex_key = BnString::new(b"00112233445566778899AABBCCDDEEFF");
    let purpose = BnString::new(b"AEAD");
    let alg = BnString::new(b"AES128");

    let mut b_kek = 0u64;
    let mut b_key = 0u64;
    assert_eq!(
        unsafe { bn_rt_crypto_bytes_from_hex(hex_kek.as_ptr(), &mut b_kek) },
        0
    );
    assert_eq!(
        unsafe { bn_rt_crypto_bytes_from_hex(hex_key.as_ptr(), &mut b_key) },
        0
    );

    let k_kek =
        unsafe { bn_rt_crypto_secret_key_from_bytes(b_kek, purpose.as_ptr(), alg.as_ptr()) };
    let k_key =
        unsafe { bn_rt_crypto_secret_key_from_bytes(b_key, purpose.as_ptr(), alg.as_ptr()) };

    let wrapped = unsafe { bn_rt_crypto_secret_key_wrap(k_kek, k_key) };
    assert_ne!(wrapped, 0);
    let hex_ptr = unsafe { bn_rt_crypto_bytes_to_hex(wrapped) };
    let hex_str = unsafe { CStr::from_ptr(hex_ptr) }
        .to_str()
        .unwrap()
        .to_uppercase();
    assert_eq!(hex_str, "1FA68B0A8112B447AEF34BD8FB5A7B829D3E862371D2CFE5");
    bn_rt_str_release(hex_ptr);

    let unwrapped = unsafe {
        bn_rt_crypto_secret_key_unwrap(k_kek, wrapped, 128, purpose.as_ptr(), alg.as_ptr())
    };
    assert_ne!(unwrapped, 0);
    assert_eq!(unsafe { bn_rt_crypto_secret_key_bits(unwrapped) }, 128);

    bn_rt_crypto_bytes_release(b_kek);
    bn_rt_crypto_bytes_release(b_key);
    bn_rt_crypto_bytes_release(wrapped);
    unsafe {
        bn_rt_crypto_secret_key_release(k_kek);
        bn_rt_crypto_secret_key_release(k_key);
        bn_rt_crypto_secret_key_release(unwrapped);
    }
}
