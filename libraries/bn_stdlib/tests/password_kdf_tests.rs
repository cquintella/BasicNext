// Basic Next password hashing (PHC Argon2id) and KDF (HKDF) tests
// Checks HashPassword, VerifyPassword, and Hkdf.

#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::similar_names,
    clippy::borrow_as_ptr
)]

use bn_stdlib::crypto::{bn_hash_password, bn_hkdf_sha256, bn_verify_password};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_from_raw, bn_rt_crypto_bytes_release, bn_rt_crypto_hash_password,
    bn_rt_crypto_hkdf, bn_rt_crypto_secret_key_bits, bn_rt_crypto_secret_key_release,
    bn_rt_crypto_verify_password,
};
use bn_stdlib::string::bn_rt_str_release;
use std::ffi::{CStr, CString};

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

    // Invalid length rejected
    assert!(bn_hkdf_sha256(&ikm, salt, info, 13).is_err());
}

#[test]
fn test_c_abi_password_and_hkdf() {
    let pw_str = CString::new("test-c-abi-password").unwrap();
    let phc_ptr = bn_rt_crypto_hash_password(pw_str.as_ptr());
    assert!(!phc_ptr.is_null());

    let phc_str = unsafe { CStr::from_ptr(phc_ptr) };
    let phc_slice = phc_str.to_str().expect("valid utf-8 phc");
    assert!(phc_slice.starts_with("$argon2id$"));

    // Verify correct password
    let match_rc = bn_rt_crypto_verify_password(pw_str.as_ptr(), phc_ptr);
    assert_eq!(match_rc, 1);

    // Verify wrong password
    let wrong_str = CString::new("wrong-pw").unwrap();
    let mismatch_rc = bn_rt_crypto_verify_password(wrong_str.as_ptr(), phc_ptr);
    assert_eq!(mismatch_rc, 0);

    bn_rt_str_release(phc_ptr);

    // Test C ABI HKDF
    let ikm_bytes = [0x33u8; 32];
    let ikm_handle = bn_rt_crypto_bytes_from_raw(ikm_bytes.as_ptr(), ikm_bytes.len());
    let salt_bytes = b"salt-bytes";
    let salt_handle = bn_rt_crypto_bytes_from_raw(salt_bytes.as_ptr(), salt_bytes.len());
    let info_bytes = b"info-label";
    let info_handle = bn_rt_crypto_bytes_from_raw(info_bytes.as_ptr(), info_bytes.len());

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
