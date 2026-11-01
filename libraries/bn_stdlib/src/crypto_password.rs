// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Password hashing (Argon2id PHC string) and Key Derivation Function (HKDF)
//! per `language/0.6/bncrypto.md`.

use std::ffi::c_char;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use hmac::{Mac, SimpleHmac};
use sha2::Sha256;

use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed};
use crate::crypto_error::CryptoFailure;
use crate::crypto_secret_abi::{SecretKeyEntry, with_secret_keys};
use crate::{c_str, c_string};

/// OWASP minimum parameters for Argon2id: m=19456 KiB (19 MiB), t=2, p=1.
const ARGON2ID_M_COST: u32 = 19456;
const ARGON2ID_T_COST: u32 = 2;
const ARGON2ID_P_COST: u32 = 1;
const ARGON2ID_OUTPUT_LEN: usize = 32;

/// Hashes `password` into a standard Argon2id PHC string.
///
/// # Errors
/// Returns `CryptoFailure::Random` if salt generation fails, or
/// `CryptoFailure::Algorithm` if hashing fails.
pub fn bn_hash_password(password: &str) -> Result<String, CryptoFailure> {
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(
        ARGON2ID_M_COST,
        ARGON2ID_T_COST,
        ARGON2ID_P_COST,
        Some(ARGON2ID_OUTPUT_LEN),
    )
    .map_err(|e| CryptoFailure::Algorithm(i32::try_from(e.to_string().len()).unwrap_or(0)))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoFailure::Algorithm(i32::try_from(e.to_string().len()).unwrap_or(0)))?;
    Ok(hash.to_string())
}

/// Verifies `password` against an Argon2id PHC string in constant time.
#[must_use]
pub fn bn_verify_password(password: &str, phc: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(phc) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// Computes HKDF-SHA256 (RFC 5869) key derivation.
///
/// # Errors
/// Returns `CryptoFailure::InvalidArgument` if `len` is not a valid key length (16, 24, 32, 64).
pub fn bn_hkdf_sha256(
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>, CryptoFailure> {
    if len != 16 && len != 24 && len != 32 && len != 64 {
        return Err(CryptoFailure::Key("invalid HKDF key length"));
    }

    // 1. Extract: PRK = HMAC-Hash(salt, IKM)
    let default_salt = [0u8; 32];
    let actual_salt = if salt.is_empty() {
        &default_salt[..]
    } else {
        salt
    };
    let mut extractor = SimpleHmac::<Sha256>::new_from_slice(actual_salt)
        .map_err(|_| CryptoFailure::Key("extract failed"))?;
    extractor.update(ikm);
    let prk = extractor.finalize().into_bytes();

    // 2. Expand: T(1) = HMAC-Hash(PRK, info || 0x01)...
    let mut okm = Vec::with_capacity(len);
    let mut previous_t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;

    while okm.len() < len {
        let mut expander = SimpleHmac::<Sha256>::new_from_slice(&prk)
            .map_err(|_| CryptoFailure::Key("expand failed"))?;
        if !previous_t.is_empty() {
            expander.update(&previous_t);
        }
        expander.update(info);
        expander.update(&[counter]);
        let t = expander.finalize().into_bytes().to_vec();
        let needed = len - okm.len();
        let to_take = needed.min(t.len());
        okm.extend_from_slice(&t[..to_take]);
        previous_t = t;
        counter = counter
            .checked_add(1)
            .ok_or(CryptoFailure::Key("counter overflow"))?;
    }

    Ok(okm)
}

// ---------------------------------------------------------------------------
// C ABI exports for password hashing and HKDF
// ---------------------------------------------------------------------------

fn get_any_key(handle: u64) -> Option<zeroize::Zeroizing<Vec<u8>>> {
    if let Some(mat) = with_secret_keys(|keys| {
        keys.get(&handle)
            .and_then(|e| (!e.zeroized).then(|| e.material.clone()))
    }) {
        return Some(mat);
    }
    if let Ok(mat) = crate::crypto_abi::get_key_material(handle) {
        return Some(mat);
    }
    crate::crypto_abi::with_buffers(|bufs| bufs.get(&handle).cloned())
}

fn store_secret_key(entry: SecretKeyEntry) -> u64 {
    let handle = crate::crypto_abi::next_handle();
    with_secret_keys(|keys| keys.insert(handle, entry));
    handle
}

fn str_from_ptr<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    if let Some(s) = c_str(ptr) {
        return Some(s);
    }
    #[allow(unsafe_code)]
    unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().ok()
}

/// C ABI: `HashPassword(password)` -> STRING.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_password(raw_pw: *const c_char) -> *mut c_char {
    let Some(password) = str_from_ptr(raw_pw) else {
        return c_string("");
    };
    match bn_hash_password(password) {
        Ok(phc) => c_string(&phc),
        Err(_) => c_string(""),
    }
}

/// C ABI: `VerifyPassword(password, phc)` -> 1 if match, 0 if mismatch or error.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_verify_password(
    raw_pw: *const c_char,
    raw_phc: *const c_char,
) -> i32 {
    let Some(password) = str_from_ptr(raw_pw) else {
        return 0;
    };
    let Some(phc_hash) = str_from_ptr(raw_phc) else {
        return 0;
    };
    i32::from(bn_verify_password(password, phc_hash))
}

/// C ABI: `Hkdf(ikm, salt, info, len) -> SecretKey OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hkdf(
    ikm_handle: u64,
    salt_handle: u64,
    info_ptr: *const c_char,
    len: i64,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(ikm_bytes) = get_any_key(ikm_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let salt_bytes = if salt_handle == 0 {
        Vec::new()
    } else {
        crate::crypto_abi::with_buffers(|bufs| bufs.get(&salt_handle).map(|b| b.to_vec()))
            .unwrap_or_default()
    };
    let info = str_from_ptr(info_ptr).unwrap_or("").as_bytes();
    let Ok(key_len) = usize::try_from(len) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };

    match bn_hkdf_sha256(&ikm_bytes, &salt_bytes, info, key_len) {
        Ok(okm) => {
            let handle = store_secret_key(SecretKeyEntry {
                material: zeroize::Zeroizing::new(okm),
                bits: key_len * 8,
                purpose: "DERIVED".to_string(),
                alg: "HKDF".to_string(),
                zeroized: false,
            });
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}
