// Basic Next crypto raw AEAD tests
// Checks that forbidden unauthenticated modes are rejected and raw AEAD requires exact lengths.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_stdlib::crypto::{Aead, generate_key_material, open, required_key_length, seal};

#[test]
fn test_unauthenticated_modes_rejected() {
    // Mode codes not in MVP (e.g. legacy ECB, unauthenticated CBC/CTR, RC4, DES) must be rejected
    for forbidden_id in [0, 8, 9, 10, 99, -1] {
        assert_eq!(required_key_length(forbidden_id), None);
        assert!(generate_key_material(forbidden_id).is_err());
    }
}

#[test]
fn test_raw_aead_nonce_and_key_lengths() {
    let key = [0x42u8; 32];
    let nonce = [0x24u8; 12];
    let plain = b"hello raw aead";
    let aad = b"auth aad";

    // Valid lengths work
    let sealed_gcm = seal(Aead::Aes256Gcm, &key, &nonce, plain, aad);
    assert!(sealed_gcm.is_some());
    let opened_gcm = open(Aead::Aes256Gcm, &key, &nonce, &sealed_gcm.unwrap(), aad);
    assert_eq!(opened_gcm.as_deref(), Some(&plain[..]));

    let sealed_chacha = seal(Aead::ChaCha20Poly1305, &key, &nonce, plain, aad);
    assert!(sealed_chacha.is_some());
    let opened_chacha = open(
        Aead::ChaCha20Poly1305,
        &key,
        &nonce,
        &sealed_chacha.unwrap(),
        aad,
    );
    assert_eq!(opened_chacha.as_deref(), Some(&plain[..]));

    // Short or long nonces fail closed (None)
    assert!(seal(Aead::Aes256Gcm, &key, &[0u8; 8], plain, aad).is_none());
    assert!(seal(Aead::Aes256Gcm, &key, &[0u8; 16], plain, aad).is_none());
    assert!(seal(Aead::ChaCha20Poly1305, &key, &[0u8; 8], plain, aad).is_none());
    assert!(seal(Aead::ChaCha20Poly1305, &key, &[0u8; 24], plain, aad).is_none());

    // Short or long keys fail closed (None)
    assert!(seal(Aead::Aes256Gcm, &[0u8; 16], &nonce, plain, aad).is_none());
    assert!(seal(Aead::ChaCha20Poly1305, &[0u8; 64], &nonce, plain, aad).is_none());
}

#[test]
fn test_raw_aead_tamper_fails_closed() {
    let key = [0x55u8; 32];
    let nonce = [0x33u8; 12];
    let plain = b"tamper test payload";
    let aad = b"context";

    let mut sealed = seal(Aead::Aes256Gcm, &key, &nonce, plain, aad).expect("seal");
    assert!(open(Aead::Aes256Gcm, &key, &nonce, &sealed, aad).is_some());

    // Tamper ciphertext
    sealed[0] ^= 0x01;
    assert_eq!(open(Aead::Aes256Gcm, &key, &nonce, &sealed, aad), None);

    // Wrong AAD
    sealed[0] ^= 0x01;
    assert_eq!(open(Aead::Aes256Gcm, &key, &nonce, &sealed, b"wrong"), None);
}
