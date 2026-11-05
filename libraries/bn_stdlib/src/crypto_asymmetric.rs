// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Asymmetric key operations (`PublicKey`, `PrivateKey`, `KeyPair`) for `BNCrypto`.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::{Mutex, PoisonError};

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::c_string;
use crate::crypto_abi::{BN_CRYPTO_INVALID_HANDLE, BN_CRYPTO_OK, next_handle};
use crate::crypto_error::CryptoFailure;

/// Ed25519 SPKI DER header (12 bytes) per RFC 8410.
pub const ED25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// X25519 SPKI DER header (12 bytes) per RFC 8410.
pub const X25519_SPKI_HEADER: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00,
];

/// ECDSA P-256 SPKI DER header (26 bytes) per RFC 5480.
pub const P256_SPKI_HEADER: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// Computes the 64-character lowercase hex `KeyId` from SPKI DER bytes.
#[must_use]
pub fn compute_spki_key_id(spki: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spki);
    crate::crypto::encode_hex(&hasher.finalize())
}

#[derive(Clone, Debug)]
pub struct PublicKeyEntry {
    pub spki: Vec<u8>,
    pub raw: Vec<u8>,
    pub algorithm: i32,
    pub key_id: String,
}

#[derive(Debug)]
pub struct PrivateKeyEntry {
    pub seed: zeroize::Zeroizing<Vec<u8>>,
    pub spki: Vec<u8>,
    pub raw_public: Vec<u8>,
    pub algorithm: i32,
    pub key_id: String,
    pub zeroized: bool,
}

impl PrivateKeyEntry {
    #[must_use]
    pub fn to_public_entry(&self) -> PublicKeyEntry {
        PublicKeyEntry {
            spki: self.spki.clone(),
            raw: self.raw_public.clone(),
            algorithm: self.algorithm,
            key_id: self.key_id.clone(),
        }
    }
}

#[derive(Default)]
struct PublicKeys {
    values: Mutex<HashMap<u64, PublicKeyEntry>>,
    holders: Mutex<HashMap<u64, usize>>,
}

#[derive(Default)]
struct PrivateKeys {
    values: Mutex<HashMap<u64, PrivateKeyEntry>>,
    holders: Mutex<HashMap<u64, usize>>,
}

pub fn with_public_keys<T>(operation: impl FnOnce(&mut HashMap<u64, PublicKeyEntry>) -> T) -> T {
    let table = bn_core_copy::table::<PublicKeys>();
    operation(&mut table.values.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn with_private_keys<T>(operation: impl FnOnce(&mut HashMap<u64, PrivateKeyEntry>) -> T) -> T {
    let table = bn_core_copy::table::<PrivateKeys>();
    operation(&mut table.values.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn store_public_key(entry: PublicKeyEntry) -> u64 {
    let handle = next_handle();
    with_public_keys(|keys| keys.insert(handle, entry));
    let table = bn_core_copy::table::<PublicKeys>();
    table
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

pub fn store_private_key(entry: PrivateKeyEntry) -> u64 {
    let handle = next_handle();
    with_private_keys(|keys| keys.insert(handle, entry));
    let table = bn_core_copy::table::<PrivateKeys>();
    table
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

fn store_keypair(
    seed: Vec<u8>,
    spki: Vec<u8>,
    raw_pub: Vec<u8>,
    algorithm: i32,
    key_id: String,
) -> (u64, u64) {
    let pub_entry = PublicKeyEntry {
        spki: spki.clone(),
        raw: raw_pub.clone(),
        algorithm,
        key_id: key_id.clone(),
    };
    let priv_entry = PrivateKeyEntry {
        seed: zeroize::Zeroizing::new(seed),
        spki,
        raw_public: raw_pub,
        algorithm,
        key_id,
        zeroized: false,
    };
    (store_public_key(pub_entry), store_private_key(priv_entry))
}

/// `Alg` selectors of the key-pair types (`modules/bn/BNCrypto.bn`, class `Alg`).
/// Only these three make a key pair; the symmetric and legacy `ALG_*` ids do not.
pub const KEY_ALG_ED25519: i32 = 30;
pub const KEY_ALG_X25519: i32 = 31;
pub const KEY_ALG_ECDSA_P256: i32 = 32;

/// Generates an Ed25519 keypair and registers both handles.
pub fn bn_ed25519_keypair_generate() -> Result<(u64, u64), CryptoFailure> {
    bn_keypair_generate_internal(KEY_ALG_ED25519)
}

/// Generates a keypair for the given algorithm ID.
pub fn bn_keypair_generate_internal(algorithm: i32) -> Result<(u64, u64), CryptoFailure> {
    match algorithm {
        KEY_ALG_ED25519 => {
            let mut seed = [0u8; 32];
            getrandom::getrandom(&mut seed)
                .map_err(|_| CryptoFailure::Random("BNCrypto.NewKeyPair"))?;
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
            let verifying_key = signing_key.verifying_key();
            let raw_pub = verifying_key.to_bytes().to_vec();

            let mut spki = Vec::with_capacity(44);
            spki.extend_from_slice(&ED25519_SPKI_HEADER);
            spki.extend_from_slice(&raw_pub);

            let key_id = compute_spki_key_id(&spki);
            let (pub_h, priv_h) = store_keypair(seed.to_vec(), spki, raw_pub, algorithm, key_id);
            Ok((pub_h, priv_h))
        }
        KEY_ALG_X25519 => {
            let mut seed = [0u8; 32];
            getrandom::getrandom(&mut seed)
                .map_err(|_| CryptoFailure::Random("BNCrypto.NewKeyPair"))?;
            let raw_pub = curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped(seed)
                .to_bytes()
                .to_vec();

            let mut spki = Vec::with_capacity(44);
            spki.extend_from_slice(&X25519_SPKI_HEADER);
            spki.extend_from_slice(&raw_pub);

            let key_id = compute_spki_key_id(&spki);
            let (pub_h, priv_h) = store_keypair(seed.to_vec(), spki, raw_pub, algorithm, key_id);
            Ok((pub_h, priv_h))
        }
        KEY_ALG_ECDSA_P256 => {
            let mut seed = [0u8; 32];
            let secret_key = loop {
                getrandom::getrandom(&mut seed)
                    .map_err(|_| CryptoFailure::Random("BNCrypto.NewKeyPair"))?;
                if let Ok(sk) = p256::SecretKey::from_slice(&seed) {
                    break sk;
                }
            };
            let signing_key = p256::ecdsa::SigningKey::from(&secret_key);
            let raw_pub = signing_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec();

            let mut spki = Vec::with_capacity(91);
            spki.extend_from_slice(&P256_SPKI_HEADER);
            spki.extend_from_slice(&raw_pub);

            let key_id = compute_spki_key_id(&spki);
            let (pub_h, priv_h) = store_keypair(seed.to_vec(), spki, raw_pub, algorithm, key_id);
            Ok((pub_h, priv_h))
        }
        _ => Err(CryptoFailure::UnsupportedAlgorithm {
            operation: "BNCrypto.NewKeyPairFor",
        }),
    }
}

/// Rebuilds a `PrivateKey` from a seed and the algorithm id stored with it.
pub fn private_entry_from_seed(
    seed: Vec<u8>,
    algorithm: i32,
) -> Result<PrivateKeyEntry, CryptoFailure> {
    let (spki, raw_public) = spki_from_seed(&seed, algorithm)?;
    let key_id = compute_spki_key_id(&spki);
    Ok(PrivateKeyEntry {
        seed: zeroize::Zeroizing::new(seed),
        spki,
        raw_public,
        algorithm,
        key_id,
        zeroized: false,
    })
}

/// Rebuilds a `PublicKey` from SPKI bytes. The header selects the algorithm.
pub fn public_entry_from_spki(spki: &[u8]) -> Result<PublicKeyEntry, CryptoFailure> {
    let (algorithm, header_len, raw_len) = if spki.starts_with(&ED25519_SPKI_HEADER) {
        (KEY_ALG_ED25519, ED25519_SPKI_HEADER.len(), 32)
    } else if spki.starts_with(&X25519_SPKI_HEADER) {
        (KEY_ALG_X25519, X25519_SPKI_HEADER.len(), 32)
    } else if spki.starts_with(&P256_SPKI_HEADER) {
        (KEY_ALG_ECDSA_P256, P256_SPKI_HEADER.len(), 65)
    } else {
        return Err(CryptoFailure::Parse {
            operation: "BNCrypto.LoadPublicKey",
        });
    };
    if spki.len() != header_len + raw_len {
        return Err(CryptoFailure::Parse {
            operation: "BNCrypto.LoadPublicKey",
        });
    }
    if algorithm == KEY_ALG_ED25519
        && !crate::crypto::ed25519_public_key_is_strong(&spki[header_len..])
    {
        return Err(CryptoFailure::Parse {
            operation: "BNCrypto.LoadPublicKey",
        });
    }
    Ok(PublicKeyEntry {
        raw: spki[header_len..].to_vec(),
        spki: spki.to_vec(),
        algorithm,
        key_id: compute_spki_key_id(spki),
    })
}

fn spki_from_seed(seed: &[u8], algorithm: i32) -> Result<(Vec<u8>, Vec<u8>), CryptoFailure> {
    let (header, raw): (&[u8], Vec<u8>) = match algorithm {
        KEY_ALG_ED25519 => {
            let raw = crate::crypto::ed25519_public_key(seed).ok_or(CryptoFailure::Parse {
                operation: "BNCrypto.LoadPrivateKey",
            })?;
            (&ED25519_SPKI_HEADER, raw)
        }
        KEY_ALG_X25519 => {
            let seed: [u8; 32] = seed.try_into().map_err(|_| CryptoFailure::Parse {
                operation: "BNCrypto.LoadPrivateKey",
            })?;
            let raw = curve25519_dalek::montgomery::MontgomeryPoint::mul_base_clamped(seed)
                .to_bytes()
                .to_vec();
            (&X25519_SPKI_HEADER, raw)
        }
        KEY_ALG_ECDSA_P256 => {
            let secret = p256::SecretKey::from_slice(seed).map_err(|_| CryptoFailure::Parse {
                operation: "BNCrypto.LoadPrivateKey",
            })?;
            let raw = p256::ecdsa::SigningKey::from(&secret)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec();
            (&P256_SPKI_HEADER, raw)
        }
        _ => {
            return Err(CryptoFailure::UnsupportedAlgorithm {
                operation: "BNCrypto.LoadPrivateKey",
            });
        }
    };
    let mut spki = Vec::with_capacity(header.len() + raw.len());
    spki.extend_from_slice(header);
    spki.extend_from_slice(&raw);
    Ok((spki, raw))
}

pub fn bn_public_key_id(handle: u64) -> Option<String> {
    with_public_keys(|keys| keys.get(&handle).map(|e| e.key_id.clone()))
}

pub fn bn_public_key_algorithm(handle: u64) -> Option<i32> {
    with_public_keys(|keys| keys.get(&handle).map(|e| e.algorithm))
}

pub fn bn_private_key_id(handle: u64) -> Option<String> {
    with_private_keys(|keys| keys.get(&handle).map(|e| e.key_id.clone()))
}

pub fn bn_private_key_algorithm(handle: u64) -> Option<i32> {
    with_private_keys(|keys| keys.get(&handle).map(|e| e.algorithm))
}

pub fn bn_private_key_public_key(handle: u64) -> Option<u64> {
    with_private_keys(|keys| {
        // Zeroize wipes the seed only; the public key holds no secret and stays
        // derivable (bncrypto.md, `Crypto.PrivateKey`).
        let pub_entry = keys.get(&handle)?.to_public_entry();
        Some(store_public_key(pub_entry))
    })
}

pub fn bn_private_key_zeroize(handle: u64) -> bool {
    with_private_keys(|keys| {
        if let Some(entry) = keys.get_mut(&handle) {
            entry.seed.as_mut_slice().zeroize();
            entry.zeroized = true;
            true
        } else {
            false
        }
    })
}

// C ABI exports

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_keypair_generate(
    alg: i32,
    out_pub: *mut u64,
    out_priv: *mut u64,
) -> i32 {
    if out_pub.is_null() || out_priv.is_null() {
        return -1;
    }
    match bn_keypair_generate_internal(alg) {
        Ok((pub_h, priv_h)) => {
            unsafe {
                *out_pub = pub_h;
                *out_priv = priv_h;
            }
            0
        }
        Err(failure) => crate::crypto_abi::failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_key_id(handle: u64) -> *mut c_char {
    bn_public_key_id(handle).map_or(std::ptr::null_mut(), |id| c_string(&id))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_key_algorithm(handle: u64) -> i32 {
    bn_public_key_algorithm(handle).unwrap_or(0)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_key_retain(handle: u64) -> i32 {
    let table = bn_core_copy::table::<PublicKeys>();
    let mut holders = table.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_CRYPTO_OK
    } else {
        BN_CRYPTO_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_key_release(handle: u64) -> i32 {
    let table = bn_core_copy::table::<PublicKeys>();
    let mut holders = table.holders.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&handle) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&handle);
        drop(holders);
        with_public_keys(|keys| {
            keys.remove(&handle);
        });
    }
    BN_CRYPTO_OK
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_id(handle: u64) -> *mut c_char {
    bn_private_key_id(handle).map_or(std::ptr::null_mut(), |id| c_string(&id))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_algorithm(handle: u64) -> i32 {
    bn_private_key_algorithm(handle).unwrap_or(0)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_public_key(handle: u64) -> u64 {
    bn_private_key_public_key(handle).unwrap_or(0)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_zeroize(handle: u64) -> i32 {
    if bn_private_key_zeroize(handle) {
        0
    } else {
        -1
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_retain(handle: u64) -> i32 {
    let table = bn_core_copy::table::<PrivateKeys>();
    let mut holders = table.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_CRYPTO_OK
    } else {
        BN_CRYPTO_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_key_release(handle: u64) -> i32 {
    let table = bn_core_copy::table::<PrivateKeys>();
    let mut holders = table.holders.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&handle) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&handle);
        drop(holders);
        with_private_keys(|keys| {
            if let Some(mut entry) = keys.remove(&handle) {
                entry.seed.as_mut_slice().zeroize();
            }
        });
    }
    BN_CRYPTO_OK
}
