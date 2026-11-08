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

/// Bytes a `DeriveKey` salt holds at least (RFC 9106 recommends 16).
pub const DERIVE_KEY_MIN_SALT: usize = 16;
/// The domain label `DeriveSubkey` puts before the context in the HKDF info.
const SUBKEY_LABEL: &[u8] = b"BNC-SUBKEY\0";
/// Catalog domain of the derived-key texts.
const CATALOG: &str = "crypto";

/// An Argon2id cost: the fields of `PasswordOptions`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordCost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl PasswordCost {
    /// The cost of `HashPassword`: the floor every Argon2id call of this module keeps.
    pub const FLOOR: Self = Self {
        memory_kib: ARGON2ID_M_COST,
        iterations: ARGON2ID_T_COST,
        parallelism: ARGON2ID_P_COST,
    };

    /// The cost of a `PasswordOptions` record; a negative field is below the floor.
    #[must_use]
    pub fn from_options(memory_kib: i64, iterations: i64, parallelism: i64) -> Option<Self> {
        Some(Self {
            memory_kib: u32::try_from(memory_kib).ok()?,
            iterations: u32::try_from(iterations).ok()?,
            parallelism: u32::try_from(parallelism).ok()?,
        })
    }

    /// The `NeedsRehash` target of a `PasswordOptions` record. No recorded cost
    /// is below a negative field, so it reads as 0: that field asks for nothing.
    #[must_use]
    pub fn target(memory_kib: i64, iterations: i64, parallelism: i64) -> Self {
        let field = |value: i64| u32::try_from(value.max(0)).unwrap_or(u32::MAX);
        Self {
            memory_kib: field(memory_kib),
            iterations: field(iterations),
            parallelism: field(parallelism),
        }
    }

    fn reaches(self, target: Self) -> bool {
        self.memory_kib >= target.memory_kib
            && self.iterations >= target.iterations
            && self.parallelism >= target.parallelism
    }

    fn argon2(self, operation: &'static str) -> Result<Argon2<'static>, CryptoFailure> {
        if !self.reaches(Self::FLOOR) {
            return Err(CryptoFailure::PasswordCost { operation });
        }
        let params = Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(ARGON2ID_OUTPUT_LEN),
        )
        .map_err(|_| CryptoFailure::PasswordCost { operation })?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

/// Hashes `password` into a standard Argon2id PHC string.
///
/// # Errors
/// As [`bn_hash_password_with`] at [`PasswordCost::FLOOR`].
pub fn bn_hash_password(password: &str) -> Result<String, CryptoFailure> {
    bn_hash_password_with(password, PasswordCost::FLOOR)
}

/// `HashPasswordWith`: an Argon2id PHC string at `cost`, with a 16-byte random salt.
///
/// # Errors
/// `PasswordCost` when `cost` is below the floor or outside Argon2id's bounds.
pub fn bn_hash_password_with(password: &str, cost: PasswordCost) -> Result<String, CryptoFailure> {
    let operation = bn_catalog::text(CATALOG, "op.hash_password_with");
    let argon2 = cost.argon2(operation)?;
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| CryptoFailure::PasswordCost { operation })?;
    Ok(hash.to_string())
}

/// `NeedsRehash`: `true` unless `phc` is an Argon2id version 19 hash whose
/// cost reaches `target`. A malformed string needs a rehash (fail-safe).
#[must_use]
pub fn bn_needs_rehash(phc: &str, target: PasswordCost) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return true;
    };
    if parsed.algorithm != Algorithm::Argon2id.ident()
        || parsed.version != Some(Version::V0x13.into())
    {
        return true;
    }
    let Ok(params) = Params::try_from(&parsed) else {
        return true;
    };
    let recorded = PasswordCost {
        memory_kib: params.m_cost(),
        iterations: params.t_cost(),
        parallelism: params.p_cost(),
    };
    !recorded.reaches(target)
}

/// `DeriveKey`: the 32-byte Argon2id output of `password` and `salt` at the
/// floor cost. Deterministic.
///
/// # Errors
/// `SaltLength` for a salt under [`DERIVE_KEY_MIN_SALT`] bytes.
pub fn bn_derive_key(
    password: &str,
    salt: &[u8],
) -> Result<zeroize::Zeroizing<Vec<u8>>, CryptoFailure> {
    let operation = bn_catalog::text(CATALOG, "op.derive_key");
    if salt.len() < DERIVE_KEY_MIN_SALT {
        return Err(CryptoFailure::SaltLength { operation });
    }
    let argon2 = PasswordCost::FLOOR.argon2(operation)?;
    let mut key = zeroize::Zeroizing::new(vec![0_u8; ARGON2ID_OUTPUT_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| CryptoFailure::PasswordCost { operation })?;
    Ok(key)
}

/// `DeriveSubkey`: HKDF-SHA256 of `master` with an empty salt and the info
/// `"BNC-SUBKEY\0" || UTF-8(context)`, 32 bytes. Deterministic.
///
/// # Errors
/// `EmptyContext` for an empty `context`.
pub fn bn_derive_subkey(
    master: &[u8],
    context: &str,
) -> Result<zeroize::Zeroizing<Vec<u8>>, CryptoFailure> {
    if context.is_empty() {
        return Err(CryptoFailure::EmptyContext {
            operation: bn_catalog::text(CATALOG, "op.derive_subkey"),
        });
    }
    let mut info = Vec::with_capacity(SUBKEY_LABEL.len() + context.len());
    info.extend_from_slice(SUBKEY_LABEL);
    info.extend_from_slice(context.as_bytes());
    bn_hkdf_sha256(master, &[], &info, ARGON2ID_OUTPUT_LEN).map(zeroize::Zeroizing::new)
}

/// The `SecretKey` entry of a derived key: purpose `DERIVED`, its algorithm
/// the derivation (`ARGON2ID` or `HKDF`), as `Hkdf` records it.
#[must_use]
pub fn derived_key_entry(
    material: zeroize::Zeroizing<Vec<u8>>,
    algorithm_key: &str,
) -> SecretKeyEntry {
    let bits = material.len() * 8;
    SecretKeyEntry {
        material,
        bits,
        purpose: bn_catalog::text(CATALOG, "key.purpose.derived").to_string(),
        alg: bn_catalog::text(CATALOG, algorithm_key).to_string(),
        zeroized: false,
    }
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
    let operation = bn_catalog::text(CATALOG, "op.hkdf");
    if len != 16 && len != 24 && len != 32 && len != 64 {
        return Err(CryptoFailure::HkdfLength { operation });
    }

    // 1. Extract: PRK = HMAC-Hash(salt, IKM)
    let default_salt = [0u8; 32];
    let actual_salt = if salt.is_empty() {
        &default_salt[..]
    } else {
        salt
    };
    let mut extractor = SimpleHmac::<Sha256>::new_from_slice(actual_salt)
        .map_err(|_| CryptoFailure::HkdfExpand { operation })?;
    extractor.update(ikm);
    let prk = extractor.finalize().into_bytes();

    // 2. Expand: T(1) = HMAC-Hash(PRK, info || 0x01)...
    let mut okm = Vec::with_capacity(len);
    let mut previous_t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;

    while okm.len() < len {
        let mut expander = SimpleHmac::<Sha256>::new_from_slice(&prk)
            .map_err(|_| CryptoFailure::HkdfExpand { operation })?;
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
            .ok_or(CryptoFailure::HkdfExpand { operation })?;
    }

    Ok(okm)
}

// ---------------------------------------------------------------------------
// C ABI exports for password hashing and HKDF
// ---------------------------------------------------------------------------

fn store_secret_key(entry: SecretKeyEntry) -> u64 {
    let handle = crate::crypto_abi::next_handle();
    with_secret_keys(|keys| keys.insert(handle, entry));
    handle
}

fn str_from_ptr<'a>(ptr: *const c_char) -> Option<&'a str> {
    // The ABI takes a BN STRING (header before the payload). There is no
    // fallback to a NUL-terminated C string: reading a header before a
    // pointer that has none is undefined, and its length would be garbage.
    c_str(ptr)
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
    info_handle: u64,
    len: i64,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let ikm_bytes =
        match crate::crypto_abi::any_key_material(ikm_handle, bn_catalog::text(CATALOG, "op.hkdf"))
        {
            Ok(material) => material,
            Err(failure) => return failed(&failure),
        };
    let salt_bytes = if salt_handle == 0 {
        Vec::new()
    } else {
        crate::crypto_abi::with_buffers(|bufs| bufs.get(&salt_handle).map(|b| b.to_vec()))
            .unwrap_or_default()
    };
    let info_bytes = if info_handle == 0 {
        Vec::new()
    } else {
        crate::crypto_abi::with_buffers(|bufs| bufs.get(&info_handle).map(|b| b.to_vec()))
            .unwrap_or_default()
    };
    let Ok(key_len) = usize::try_from(len) else {
        return failed(&CryptoFailure::HkdfLength {
            operation: bn_catalog::text(CATALOG, "op.hkdf"),
        });
    };

    match bn_hkdf_sha256(&ikm_bytes, &salt_bytes, &info_bytes, key_len) {
        Ok(okm) => {
            let handle = store_secret_key(SecretKeyEntry {
                material: zeroize::Zeroizing::new(okm),
                bits: key_len * 8,
                purpose: "DERIVED".to_string(),
                alg: "HKDF".to_string(),
                zeroized: false,
            });
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            // SAFETY: `out_handle` was checked non-null at entry; it is the caller's slot.
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// Writes the derived key `result` as a new `SecretKey` handle through `out`.
#[allow(unsafe_code)] // C ABI: writes the caller's out slot.
fn derived_key_status(
    out: *mut i64,
    result: Result<zeroize::Zeroizing<Vec<u8>>, CryptoFailure>,
    algorithm_key: &str,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    match result {
        Ok(material) => {
            let handle = store_secret_key(derived_key_entry(material, algorithm_key));
            // SAFETY: `out` was checked non-null; it is the caller's slot.
            unsafe { *out = i64::try_from(handle).unwrap_or(i64::MAX) };
            BN_CRYPTO_OK
        }
        Err(failure) => failed(&failure),
    }
}

/// C ABI: `HashPasswordWith(password, opts) AS STRING OR Error`; emitted code
/// passes the three `PasswordOptions` fields.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_password_with(
    raw_pw: *const c_char,
    memory_kib: i32,
    iterations: i32,
    parallelism: i32,
    out: *mut *mut c_char,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let operation = bn_catalog::text(CATALOG, "op.hash_password_with");
    let result = str_from_ptr(raw_pw)
        .ok_or(CryptoFailure::Utf8)
        .and_then(|password| {
            let cost = PasswordCost::from_options(
                i64::from(memory_kib),
                i64::from(iterations),
                i64::from(parallelism),
            )
            .ok_or(CryptoFailure::PasswordCost { operation })?;
            bn_hash_password_with(password, cost)
        });
    match result {
        Ok(phc) => {
            // SAFETY: `out` was checked non-null; it is the caller's slot.
            unsafe { *out = c_string(&phc) };
            BN_CRYPTO_OK
        }
        Err(failure) => failed(&failure),
    }
}

/// C ABI: `NeedsRehash(phc, opts)`: 1 when a rehash is needed, else 0. A
/// negative target field is below every recorded cost: no rehash for it.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_needs_rehash(
    raw_phc: *const c_char,
    memory_kib: i32,
    iterations: i32,
    parallelism: i32,
) -> i32 {
    let Some(phc) = str_from_ptr(raw_phc) else {
        return 1;
    };
    let target = PasswordCost::target(
        i64::from(memory_kib),
        i64::from(iterations),
        i64::from(parallelism),
    );
    i32::from(bn_needs_rehash(phc, target))
}

/// C ABI: `DeriveKey(password, salt) AS SecretKey OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_derive_key(raw_pw: *const c_char, salt: u64, out: *mut i64) -> i32 {
    let operation = bn_catalog::text(CATALOG, "op.derive_key");
    let result = str_from_ptr(raw_pw)
        .ok_or(CryptoFailure::Utf8)
        .and_then(|password| {
            let salt = crate::crypto_abi::with_buffers(|buffers| {
                buffers.get(&salt).map(|bytes| bytes.to_vec())
            })
            .ok_or(CryptoFailure::Key(operation))?;
            bn_derive_key(password, &salt)
        });
    derived_key_status(out, result, "key.alg.argon2id")
}

/// C ABI: `DeriveSubkey(master, context) AS SecretKey OR Error`. A zeroized
/// master is `ZeroizedKey` (`INVALID_ARGUMENT`).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_derive_subkey(
    master: u64,
    raw_context: *const c_char,
    out: *mut i64,
) -> i32 {
    let result =
        crate::crypto_abi::any_key_material(master, bn_catalog::text(CATALOG, "op.derive_subkey"))
            .and_then(|material| {
                let context = str_from_ptr(raw_context).ok_or(CryptoFailure::Utf8)?;
                bn_derive_subkey(&material, context)
            });
    derived_key_status(out, result, "key.alg.hkdf")
}
