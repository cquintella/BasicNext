// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_stdlib::crypto::{bn_decrypt_envelope, bn_encrypt_envelope};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_text, bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release,
    bn_rt_crypto_decrypt, bn_rt_crypto_decrypt_text, bn_rt_crypto_encrypt,
    bn_rt_crypto_encrypt_text, bn_rt_crypto_secret_key_generate, bn_rt_crypto_secret_key_release,
    bn_rt_str_release,
};

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

    let text = std::ffi::CString::new("Sensitive payload data").unwrap();
    let text_handle = bn_rt_crypto_bytes_from_text(text.as_ptr());
    assert!(text_handle > 0);

    let sealed_handle = bn_rt_crypto_encrypt(key_handle, text_handle);
    assert!(sealed_handle > 0);
    let sealed_len = bn_rt_crypto_bytes_length(sealed_handle);
    assert_eq!(sealed_len, 31 + 22 + 16);

    let decrypted_handle = bn_rt_crypto_decrypt(key_handle, sealed_handle);
    assert!(decrypted_handle > 0);
    assert_eq!(bn_rt_crypto_bytes_length(decrypted_handle), 22);

    // Text helper C ABI
    let token_ptr = bn_rt_crypto_encrypt_text(key_handle, text.as_ptr());
    assert!(!token_ptr.is_null());

    let recovered_ptr = bn_rt_crypto_decrypt_text(key_handle, token_ptr);
    assert!(!recovered_ptr.is_null());
    // SAFETY: recovered_ptr is null-terminated C string from runtime.
    let recovered_str = unsafe { std::ffi::CStr::from_ptr(recovered_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(recovered_str, "Sensitive payload data");

    // Clean up
    unsafe {
        bn_rt_str_release(token_ptr);
        bn_rt_str_release(recovered_ptr);
    }
    bn_rt_crypto_bytes_release(text_handle);
    bn_rt_crypto_bytes_release(sealed_handle);
    bn_rt_crypto_bytes_release(decrypted_handle);
    bn_rt_crypto_secret_key_release(key_handle);
}
