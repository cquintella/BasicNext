// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI handle table for `BNCrypto.Secret` and `BNCrypto.SecretKey` in compiled binaries.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::{Mutex, PoisonError};

use aes_gcm::aes::cipher::{BlockDecrypt, BlockEncrypt};
use aes_gcm::aes::{Aes128, Aes192, Aes256};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::crypto_abi::{BN_CRYPTO_INVALID_HANDLE, BN_CRYPTO_OK, next_handle, store, with_buffers};
use crate::crypto_error::CryptoFailure;
use crate::{c_str, c_string};

/// `KeyId` for `SecretKey`: SHA-256 of `("BNC-KEYID\0" || raw_bytes)` formatted as 64-character lowercase hex.
#[must_use]
pub fn compute_secret_key_id(material: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"BNC-KEYID\0");
    hasher.update(material);
    crate::crypto::encode_hex(&hasher.finalize())
}

const RFC3394_IV: [u8; 8] = [0xA6, 0xA6, 0xA6, 0xA6, 0xA6, 0xA6, 0xA6, 0xA6];

enum AesCipher {
    Aes128(Aes128),
    Aes192(Aes192),
    Aes256(Aes256),
}

impl AesCipher {
    fn new(kek: &[u8]) -> Option<Self> {
        use aes_gcm::aes::cipher::KeyInit;
        match kek.len() {
            16 => Aes128::new_from_slice(kek).ok().map(AesCipher::Aes128),
            24 => Aes192::new_from_slice(kek).ok().map(AesCipher::Aes192),
            32 => Aes256::new_from_slice(kek).ok().map(AesCipher::Aes256),
            _ => None,
        }
    }

    fn encrypt_block(&self, block: &mut [u8; 16]) {
        let b = aes_gcm::aes::cipher::generic_array::GenericArray::from_mut_slice(block);
        match self {
            AesCipher::Aes128(c) => c.encrypt_block(b),
            AesCipher::Aes192(c) => c.encrypt_block(b),
            AesCipher::Aes256(c) => c.encrypt_block(b),
        }
    }

    fn decrypt_block(&self, block: &mut [u8; 16]) {
        let b = aes_gcm::aes::cipher::generic_array::GenericArray::from_mut_slice(block);
        match self {
            AesCipher::Aes128(c) => c.decrypt_block(b),
            AesCipher::Aes192(c) => c.decrypt_block(b),
            AesCipher::Aes256(c) => c.decrypt_block(b),
        }
    }
}

/// RFC 3394 AES Key Wrap.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn rfc3394_key_wrap(kek: &[u8], key_data: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    let cipher = AesCipher::new(kek).ok_or(CryptoFailure::KeyLength {
        expected: 32,
        actual: kek.len(),
    })?;
    if key_data.len() < 16 || !key_data.len().is_multiple_of(8) {
        return Err(CryptoFailure::KeyWrapLength);
    }
    let n = key_data.len() / 8;
    let mut a = RFC3394_IV;
    let mut r: Vec<[u8; 8]> = Vec::with_capacity(n);
    for chunk in key_data.chunks_exact(8) {
        let mut block = [0u8; 8];
        block.copy_from_slice(chunk);
        r.push(block);
    }

    let mut buf = [0u8; 16];
    for j in 0..6 {
        for i in 1..=n {
            buf[..8].copy_from_slice(&a);
            buf[8..].copy_from_slice(&r[i - 1]);
            cipher.encrypt_block(&mut buf);
            let t = (n * j + i) as u64;
            let mut msb = [0u8; 8];
            msb.copy_from_slice(&buf[..8]);
            let msb_int = u64::from_be_bytes(msb) ^ t;
            a = msb_int.to_be_bytes();
            r[i - 1].copy_from_slice(&buf[8..]);
        }
    }

    let mut out = Vec::with_capacity((n + 1) * 8);
    out.extend_from_slice(&a);
    for block in r {
        out.extend_from_slice(&block);
    }
    Ok(out)
}

/// RFC 3394 AES Key Unwrap.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn rfc3394_key_unwrap(kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    let cipher = AesCipher::new(kek).ok_or(CryptoFailure::KeyLength {
        expected: 32,
        actual: kek.len(),
    })?;
    if wrapped.len() < 24 || !wrapped.len().is_multiple_of(8) {
        return Err(CryptoFailure::KeyWrapLength);
    }
    let n = (wrapped.len() / 8) - 1;
    let mut a = [0u8; 8];
    a.copy_from_slice(&wrapped[..8]);
    let mut r: Vec<[u8; 8]> = Vec::with_capacity(n);
    for chunk in wrapped[8..].chunks_exact(8) {
        let mut block = [0u8; 8];
        block.copy_from_slice(chunk);
        r.push(block);
    }

    let mut buf = [0u8; 16];
    for j in (0..6).rev() {
        for i in (1..=n).rev() {
            let t = (n * j + i) as u64;
            let a_int = u64::from_be_bytes(a) ^ t;
            buf[..8].copy_from_slice(&a_int.to_be_bytes());
            buf[8..].copy_from_slice(&r[i - 1]);
            cipher.decrypt_block(&mut buf);
            a.copy_from_slice(&buf[..8]);
            r[i - 1].copy_from_slice(&buf[8..]);
        }
    }

    if a.as_slice().ct_eq(&RFC3394_IV).into() {
        let mut out = zeroize::Zeroizing::new(Vec::with_capacity(n * 8));
        for block in r {
            out.extend_from_slice(&block);
        }
        Ok(out.to_vec())
    } else {
        Err(CryptoFailure::KeyUnwrapAuth)
    }
}

/// The secret buffers of the module copy that runs on this thread.
#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<u64, zeroize::Zeroizing<Vec<u8>>>>,
    holders: Mutex<HashMap<u64, usize>>,
}

fn with_secrets<T>(
    operation: impl FnOnce(&mut HashMap<u64, zeroize::Zeroizing<Vec<u8>>>) -> T,
) -> T {
    let secrets = bn_core_copy::table::<Secrets>();
    operation(
        &mut secrets
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn store_secret(bytes: Vec<u8>) -> u64 {
    let handle = next_handle();
    with_secrets(|secrets| secrets.insert(handle, zeroize::Zeroizing::new(bytes)));
    let secrets = bn_core_copy::table::<Secrets>();
    secrets
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_from_text(text: *const c_char) -> u64 {
    store_secret(
        c_str(text)
            .map(|text| text.as_bytes().to_vec())
            .unwrap_or_default(),
    )
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_length(handle: u64) -> i64 {
    with_secrets(|secrets| {
        secrets
            .get(&handle)
            .and_then(|bytes| i64::try_from(bytes.len()).ok())
            .unwrap_or(-1)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_to_bytes(handle: u64) -> u64 {
    let bytes = with_secrets(|secrets| secrets.get(&handle).map(|b| b.to_vec()));
    bytes.map_or(0, store)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_constant_time_equals(a: u64, b: u64) -> i32 {
    let a_bytes = with_secrets(|secrets| secrets.get(&a).cloned());
    let b_bytes = with_secrets(|secrets| secrets.get(&b).cloned());
    match (a_bytes, b_bytes) {
        (Some(a_buf), Some(b_buf)) => {
            i32::from(crate::crypto::constant_time_equals(&a_buf, &b_buf))
        }
        _ => 0,
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_zeroize(handle: u64) -> i32 {
    with_secrets(|secrets| {
        if let Some(buf) = secrets.get_mut(&handle) {
            buf.as_mut_slice().zeroize();
            BN_CRYPTO_OK
        } else {
            BN_CRYPTO_INVALID_HANDLE
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_retain(handle: u64) -> i32 {
    let secrets = bn_core_copy::table::<Secrets>();
    let mut holders = secrets
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_CRYPTO_OK
    } else {
        BN_CRYPTO_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_release(handle: u64) -> i32 {
    let secrets = bn_core_copy::table::<Secrets>();
    let mut holders = secrets
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&handle) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&handle);
        drop(holders);
        with_secrets(|secrets| {
            if let Some(mut buf) = secrets.remove(&handle) {
                buf.as_mut_slice().zeroize();
            }
        });
    }
    BN_CRYPTO_OK
}

/// The secret key representation for the C ABI runtime.
#[derive(Debug)]
pub struct SecretKeyEntry {
    pub material: zeroize::Zeroizing<Vec<u8>>,
    pub bits: usize,
    pub purpose: String,
    pub alg: String,
    pub zeroized: bool,
}

#[derive(Default)]
struct SecretKeys {
    values: Mutex<HashMap<u64, SecretKeyEntry>>,
    holders: Mutex<HashMap<u64, usize>>,
}

pub(crate) fn with_secret_keys<T>(
    operation: impl FnOnce(&mut HashMap<u64, SecretKeyEntry>) -> T,
) -> T {
    let keys = bn_core_copy::table::<SecretKeys>();
    operation(&mut keys.values.lock().unwrap_or_else(PoisonError::into_inner))
}

fn store_secret_key(entry: SecretKeyEntry) -> u64 {
    let handle = next_handle();
    with_secret_keys(|keys| keys.insert(handle, entry));
    let keys = bn_core_copy::table::<SecretKeys>();
    keys.holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_generate(
    bits: i64,
    purpose: *const c_char,
    alg: *const c_char,
) -> u64 {
    let num_bits = usize::try_from(bits).unwrap_or(256);
    let byte_len = if num_bits == 0 { 32 } else { num_bits / 8 };
    let mut material = vec![0u8; byte_len];
    if getrandom::getrandom(&mut material).is_err() {
        return 0;
    }
    let purpose_str = c_str(purpose).unwrap_or("AEAD").to_string();
    let alg_str = c_str(alg).unwrap_or("AES256").to_string();
    store_secret_key(SecretKeyEntry {
        material: zeroize::Zeroizing::new(material),
        bits: if num_bits == 0 { 256 } else { num_bits },
        purpose: purpose_str,
        alg: alg_str,
        zeroized: false,
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_from_bytes(
    bytes_handle: u64,
    purpose: *const c_char,
    alg: *const c_char,
) -> u64 {
    let bytes = match with_buffers(|b| b.get(&bytes_handle).cloned()) {
        Some(b) => b.to_vec(),
        None => return 0,
    };
    let bits = bytes.len() * 8;
    let purpose_str = c_str(purpose).unwrap_or("AEAD").to_string();
    let alg_str = c_str(alg).unwrap_or("AES256").to_string();
    store_secret_key(SecretKeyEntry {
        material: zeroize::Zeroizing::new(bytes),
        bits,
        purpose: purpose_str,
        alg: alg_str,
        zeroized: false,
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_id(handle: u64) -> *mut c_char {
    with_secret_keys(|keys| {
        keys.get(&handle).map_or(std::ptr::null_mut(), |entry| {
            let id = compute_secret_key_id(&entry.material);
            c_string(&id)
        })
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_bits(handle: u64) -> i64 {
    with_secret_keys(|keys| {
        keys.get(&handle).map_or(-1, |entry| {
            if entry.zeroized {
                0
            } else {
                i64::try_from(entry.bits).unwrap_or(-1)
            }
        })
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_length(handle: u64) -> i64 {
    with_secret_keys(|keys| {
        keys.get(&handle).map_or(-1, |entry| {
            if entry.zeroized {
                0
            } else {
                i64::try_from(entry.material.len()).unwrap_or(-1)
            }
        })
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_purpose(handle: u64) -> *mut c_char {
    with_secret_keys(|keys| {
        keys.get(&handle)
            .map_or(std::ptr::null_mut(), |entry| c_string(&entry.purpose))
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_algorithm(handle: u64) -> *mut c_char {
    with_secret_keys(|keys| {
        keys.get(&handle)
            .map_or(std::ptr::null_mut(), |entry| c_string(&entry.alg))
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_export(handle: u64, format: *const c_char) -> u64 {
    let bytes = with_secret_keys(|keys| keys.get(&handle).map(|entry| entry.material.to_vec()));
    let Some(material) = bytes else {
        return 0;
    };
    let fmt = c_str(format).unwrap_or("RAW");
    if fmt == "HEX" {
        let hex = crate::crypto::encode_hex(&material);
        store(hex.into_bytes())
    } else {
        store(material)
    }
}

#[allow(unsafe_code, clippy::similar_names)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_wrap(kek_handle: u64, key_handle: u64) -> u64 {
    let kek_bytes = with_secret_keys(|keys| keys.get(&kek_handle).map(|e| e.material.to_vec()));
    let key_bytes = with_secret_keys(|keys| keys.get(&key_handle).map(|e| e.material.to_vec()));
    let (Some(kek), Some(key_data)) = (kek_bytes, key_bytes) else {
        return 0;
    };
    match rfc3394_key_wrap(&kek, &key_data) {
        Ok(wrapped) => store(wrapped),
        Err(_) => 0,
    }
}

#[allow(unsafe_code, clippy::similar_names)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_unwrap(
    kek_handle: u64,
    wrapped_handle: u64,
    bits: i64,
    purpose: *const c_char,
    alg: *const c_char,
) -> u64 {
    let kek_bytes = with_secret_keys(|keys| keys.get(&kek_handle).map(|e| e.material.to_vec()));
    let wrapped_bytes = match with_buffers(|b| b.get(&wrapped_handle).cloned()) {
        Some(b) => b.to_vec(),
        None => return 0,
    };
    let Some(kek) = kek_bytes else {
        return 0;
    };
    match rfc3394_key_unwrap(&kek, &wrapped_bytes) {
        Ok(unwrapped) => {
            let num_bits = if bits <= 0 {
                unwrapped.len() * 8
            } else {
                usize::try_from(bits).unwrap_or(256)
            };
            let purpose_str = c_str(purpose).unwrap_or("AEAD").to_string();
            let alg_str = c_str(alg).unwrap_or("AES256").to_string();
            store_secret_key(SecretKeyEntry {
                material: zeroize::Zeroizing::new(unwrapped),
                bits: num_bits,
                purpose: purpose_str,
                alg: alg_str,
                zeroized: false,
            })
        }
        Err(_) => 0,
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_zeroize(handle: u64) -> i32 {
    with_secret_keys(|keys| {
        if let Some(entry) = keys.get_mut(&handle) {
            entry.material.as_mut_slice().zeroize();
            entry.zeroized = true;
            BN_CRYPTO_OK
        } else {
            BN_CRYPTO_INVALID_HANDLE
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_retain(handle: u64) -> i32 {
    let keys = bn_core_copy::table::<SecretKeys>();
    let mut holders = keys.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_CRYPTO_OK
    } else {
        BN_CRYPTO_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_secret_key_release(handle: u64) -> i32 {
    let keys = bn_core_copy::table::<SecretKeys>();
    let mut holders = keys.holders.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&handle) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&handle);
        drop(holders);
        with_secret_keys(|keys| {
            if let Some(mut entry) = keys.remove(&handle) {
                entry.material.as_mut_slice().zeroize();
            }
        });
    }
    BN_CRYPTO_OK
}
