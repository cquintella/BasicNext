// Basic Next password hashing (PHC Argon2id) and KDF (HKDF) tests
// Checks HashPassword, VerifyPassword, and Hkdf.

#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::similar_names,
    clippy::borrow_as_ptr,
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_stdlib::crypto::{bn_hash_password, bn_hkdf_sha256, bn_verify_password};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_release, bn_rt_crypto_hash_password, bn_rt_crypto_hkdf,
    bn_rt_crypto_secret_key_bits, bn_rt_crypto_secret_key_release, bn_rt_crypto_verify_password,
};
use bn_stdlib::string::bn_rt_str_release;
use std::ffi::CStr;

/// A live `Bytes` handle holding `bytes`, made through the reachable C ABI.
fn buffer(bytes: &[u8]) -> u64 {
    let hex =
        bn_stdlib::string::c_string_from_bytes(bn_stdlib::crypto::encode_hex(bytes).as_bytes());
    let mut handle = 0_u64;
    let rc = bn_stdlib::crypto_abi::bn_rt_crypto_bytes_from_hex(hex, &raw mut handle);
    bn_stdlib::string::bn_rt_str_release(hex);
    assert_eq!(rc, 0, "the call succeeds");
    handle
}

#[test]
fn test_password_hash_and_verification() {
    let password = "correct-horse-battery-staple";
    let phc = bn_hash_password(password).expect("hash password");

    // Must be Argon2id with OWASP parameters (m=19456, t=2, p=1)
    assert!(
        phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "phc format: {phc}"
    );

    // Correct password matches
    assert!(bn_verify_password(password, &phc));

    // Wrong password fails
    assert!(!bn_verify_password("wrong-password", &phc));
}

#[test]
fn test_hkdf_key_derivation() {
    let ikm = [0x11u8; 32];
    let salt = b"salt-for-hkdf";
    let info = b"application-info-context";

    let okm = bn_hkdf_sha256(&ikm, salt, info, 32).expect("hkdf 32 bytes");
    assert_eq!(okm.len(), 32);

    let okm_repeat = bn_hkdf_sha256(&ikm, salt, info, 32).expect("hkdf repeat");
    assert_eq!(okm, okm_repeat);

    // Invalid length rejected with catalog operation and message
    let err = bn_hkdf_sha256(&ikm, salt, info, 13).unwrap_err();
    assert_eq!(err.code(), bn_types::error_codes::crypto::INVALID_ARGUMENT);
    assert_eq!(err.operation(), "BNCrypto.Hkdf");
    assert_eq!(
        err.message(),
        "derived key length must be 16, 24, 32, or 64 bytes"
    );
}

#[test]
fn test_c_abi_password_and_hkdf() {
    let pw_str = bn_stdlib::string::c_string_from_bytes(b"test-c-abi-password");
    let phc_ptr = bn_rt_crypto_hash_password(pw_str);
    assert!(!phc_ptr.is_null());

    // SAFETY: the runtime returned this NUL-terminated STRING, alive until it is released.

    let phc_str = unsafe { CStr::from_ptr(phc_ptr) };
    let phc_slice = phc_str.to_str().expect("valid utf-8 phc");
    assert!(phc_slice.starts_with("$argon2id$"));

    // Verify correct password
    let match_rc = bn_rt_crypto_verify_password(pw_str, phc_ptr);
    assert_eq!(match_rc, 1);

    // Verify wrong password
    let wrong_str = bn_stdlib::string::c_string_from_bytes(b"wrong-pw");
    let mismatch_rc = bn_rt_crypto_verify_password(wrong_str, phc_ptr);
    assert_eq!(mismatch_rc, 0);

    bn_rt_str_release(phc_ptr);
    bn_rt_str_release(pw_str);
    bn_rt_str_release(wrong_str);

    // Test C ABI HKDF
    let ikm_bytes = [0x33u8; 32];
    let ikm_handle = buffer(&ikm_bytes);
    let salt_bytes = b"salt-bytes";
    let salt_handle = buffer(salt_bytes);
    let info_bytes = b"info-label";
    let info_handle = buffer(info_bytes);

    let mut out_key_handle = 0i64;
    let rc_hkdf = bn_rt_crypto_hkdf(
        ikm_handle,
        salt_handle,
        info_handle,
        32,
        &raw mut out_key_handle,
    );
    assert_eq!(rc_hkdf, 0);
    assert_ne!(out_key_handle, 0);

    let bits = bn_rt_crypto_secret_key_bits(out_key_handle.cast_unsigned());
    assert_eq!(bits, 256);

    let _ = bn_rt_crypto_bytes_release(ikm_handle);
    let _ = bn_rt_crypto_bytes_release(salt_handle);
    let _ = bn_rt_crypto_bytes_release(info_handle);
    let _ = bn_rt_crypto_secret_key_release(out_key_handle.cast_unsigned());
}
