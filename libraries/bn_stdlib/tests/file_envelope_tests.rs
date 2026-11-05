// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(unsafe_code)]

use bn_stdlib::crypto::{bn_decrypt_file, bn_encrypt_file};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_decrypt_file, bn_rt_crypto_encrypt_file, bn_rt_crypto_secret_key_generate,
    bn_rt_crypto_secret_key_release,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use std::ffi::c_char;
use std::fs;

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

fn temp_file_path(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("bn_test_{name}_{nonce}_{}.tmp", std::process::id()));
    dir
}

#[test]
fn test_file_encryption_roundtrip_sizes() {
    let key = [0x5au8; 32];

    // Test cases: 0 bytes (empty), 128 bytes, 64 KiB, and 150 KiB (multi-chunk)
    let test_sizes = [0usize, 128, 64 * 1024, 150 * 1024];

    for &size in &test_sizes {
        let in_path = temp_file_path(&format!("in_{size}"));
        let enc_path = temp_file_path(&format!("enc_{size}"));
        let out_path = temp_file_path(&format!("out_{size}"));

        let mut data = vec![0u8; size];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = u8::try_from(i % 251).expect("within u8 range");
        }
        fs::write(&in_path, &data).expect("write plain input file");

        // 1. Encrypt file
        bn_encrypt_file(
            &bn_stdlib::FsPolicy::unrestricted(),
            &key,
            &in_path,
            &enc_path,
        )
        .expect("encrypt file");
        assert!(enc_path.exists(), "encrypted file must exist");

        let enc_bytes = fs::read(&enc_path).expect("read encrypted file");
        assert!(
            enc_bytes.starts_with(b"BNC1"),
            "encrypted file must start with BNC1 magic"
        );

        // 2. Decrypt file
        bn_decrypt_file(
            &bn_stdlib::FsPolicy::unrestricted(),
            &key,
            &enc_path,
            &out_path,
        )
        .expect("decrypt file");
        assert!(out_path.exists(), "decrypted file must exist");

        let decrypted = fs::read(&out_path).expect("read decrypted file");
        assert_eq!(
            decrypted, data,
            "decrypted content must match original for size {size}"
        );

        let _ = fs::remove_file(&in_path);
        let _ = fs::remove_file(&enc_path);
        let _ = fs::remove_file(&out_path);
    }
}

#[test]
fn test_file_encryption_fails_if_output_exists() {
    let key = [0x33u8; 32];
    let in_path = temp_file_path("exists_in");
    let enc_path = temp_file_path("exists_enc");

    fs::write(&in_path, b"some file content").unwrap();
    fs::write(&enc_path, b"already exists").unwrap();

    // Encrypting to an existing file must fail (FS.OpenNew semantics)
    let res = bn_encrypt_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &key,
        &in_path,
        &enc_path,
    );
    assert!(res.is_err(), "must fail when destination already exists");

    let _ = fs::remove_file(&in_path);
    let _ = fs::remove_file(&enc_path);
}

#[test]
fn test_file_decryption_deletes_partial_output_on_tamper() {
    let key = [0x77u8; 32];
    let in_path = temp_file_path("tamper_in");
    let enc_path = temp_file_path("tamper_enc");
    let out_path = temp_file_path("tamper_out");

    // Multi-chunk file: 140 KiB
    let mut data = vec![0u8; 140 * 1024];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = u8::try_from(i % 255).expect("within u8 range");
    }
    fs::write(&in_path, &data).unwrap();

    bn_encrypt_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &key,
        &in_path,
        &enc_path,
    )
    .expect("encrypt file");

    // Tamper byte in 2nd chunk
    let mut enc_bytes = fs::read(&enc_path).unwrap();
    let tamper_idx = 31 + 65536 + 100;
    enc_bytes[tamper_idx] ^= 0x42;
    fs::write(&enc_path, &enc_bytes).unwrap();

    // Decrypt must fail
    let res = bn_decrypt_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &key,
        &enc_path,
        &out_path,
    );
    assert!(res.is_err(), "decryption of tampered file must fail");

    // Partial output file MUST be deleted!
    assert!(
        !out_path.exists(),
        "partial output file must be deleted upon decryption failure"
    );

    let _ = fs::remove_file(&in_path);
    let _ = fs::remove_file(&enc_path);
    let _ = fs::remove_file(&out_path);
}

#[test]
fn test_file_c_abi_roundtrip() {
    let in_path = temp_file_path("c_abi_in");
    let enc_path = temp_file_path("c_abi_enc");
    let out_path = temp_file_path("c_abi_out");

    let data = b"Testing C ABI streaming file encryption and decryption!";
    fs::write(&in_path, data).unwrap();

    let key_handle = bn_rt_crypto_secret_key_generate(256, std::ptr::null(), std::ptr::null());
    assert_ne!(key_handle, 0, "generate secret key");

    let in_str = BnString::new(in_path.to_str().unwrap().as_bytes());
    let enc_str = BnString::new(enc_path.to_str().unwrap().as_bytes());
    let out_str = BnString::new(out_path.to_str().unwrap().as_bytes());

    let enc_rc = bn_rt_crypto_encrypt_file(key_handle, in_str.as_ptr(), enc_str.as_ptr());
    assert_eq!(enc_rc, 0, "C ABI encrypt file must succeed");
    assert!(enc_path.exists());

    let dec_rc = bn_rt_crypto_decrypt_file(key_handle, enc_str.as_ptr(), out_str.as_ptr());
    assert_eq!(dec_rc, 0, "C ABI decrypt file must succeed");
    assert!(out_path.exists());

    let recovered = fs::read(&out_path).unwrap();
    assert_eq!(recovered, data);

    assert_eq!(bn_rt_crypto_secret_key_release(key_handle), 0);
    let _ = fs::remove_file(&in_path);
    let _ = fs::remove_file(&enc_path);
    let _ = fs::remove_file(&out_path);
}
