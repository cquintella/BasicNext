// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Detached signatures and PKCS#8 PEM for `BNCrypto` asymmetric keys.
//! One implementation serves the interpreter and the C ABI.

use std::ffi::c_char;
use std::io::Read;

use pkcs8::DecodePrivateKey;
use pkcs8::EncodePrivateKey;
use pkcs8::der::{Decode, Encode};

use crate::crypto::{ed25519_sign, ed25519_verify, p256_sign, p256_verify};
use crate::crypto_abi::{failed, store, with_buffers};
use crate::crypto_asymmetric::{
    private_entry_from_seed, public_entry_from_spki, store_private_key, store_public_key,
    with_private_keys, with_public_keys,
};
use crate::crypto_encoding::bn_pem_encode;
use crate::crypto_error::CryptoFailure;
use crate::{c_str, c_string};

/// `PemFormat.PKCS8` in `modules/bn/BNCrypto.bn`. Any other value is rejected.
pub const PEM_FORMAT_PKCS8: i32 = 1;

const LABEL_PRIVATE: &str = "PRIVATE KEY";
const LABEL_ENCRYPTED: &str = "ENCRYPTED PRIVATE KEY";
const LABEL_PUBLIC: &str = "PUBLIC KEY";
const SIGN: &str = "BNCrypto.Sign";
const SIGN_FILE: &str = "BNCrypto.SignFile";
pub const EXPORT: &str = "BNCrypto.ExportPem";
pub const EXPORT_PLAIN: &str = "BNCrypto.ExportPemUnencrypted";
const LOAD_PRIVATE: &str = "BNCrypto.LoadPrivateKey";
const LOAD_PUBLIC: &str = "BNCrypto.LoadPublicKey";

struct KeyMaterial {
    seed: Vec<u8>,
    algorithm: i32,
}

fn parse(operation: &'static str) -> CryptoFailure {
    CryptoFailure::Parse { operation }
}

fn private_material(handle: u64, operation: &'static str) -> Result<KeyMaterial, CryptoFailure> {
    with_private_keys(|keys| {
        let Some(entry) = keys.get(&handle) else {
            return Err(CryptoFailure::Key(operation));
        };
        if entry.zeroized {
            return Err(CryptoFailure::ZeroizedKey);
        }
        Ok(KeyMaterial {
            seed: entry.seed.to_vec(),
            algorithm: entry.algorithm,
        })
    })
}

fn public_material(handle: u64) -> Option<(Vec<u8>, i32)> {
    with_public_keys(|keys| {
        keys.get(&handle)
            .map(|entry| (entry.raw.clone(), entry.algorithm))
    })
}

/// Detached signature. Ed25519 is RFC 8032; ECDSA P-256 is RFC 6979 raw `r || s`.
/// X25519 is not a signature algorithm.
pub fn bn_sign(seed: &[u8], algorithm: i32, message: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    match algorithm {
        1 | 4 | 30 => ed25519_sign(seed, message).ok_or(CryptoFailure::Key(SIGN)),
        5 | 32 => p256_sign(seed, message).ok_or(CryptoFailure::Key(SIGN)),
        31 => Err(CryptoFailure::WrongKeyType { operation: SIGN }),
        _ => Err(CryptoFailure::UnsupportedAlgorithm { operation: SIGN }),
    }
}

/// `FALSE` for a bad signature, a mismatched key, or an algorithm that does not sign.
#[must_use]
pub fn bn_verify(raw_public: &[u8], algorithm: i32, message: &[u8], signature: &[u8]) -> bool {
    match algorithm {
        1 | 4 | 30 => ed25519_verify(raw_public, message, signature),
        5 | 32 => p256_verify(raw_public, message, signature),
        _ => false,
    }
}

fn read_file(path: &str, operation: &'static str) -> Result<Vec<u8>, CryptoFailure> {
    let mut file =
        crate::policy::open_path(std::path::Path::new(path), crate::secure_fs::OpenMode::Read)
            .map_err(|_| CryptoFailure::Io { operation })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| CryptoFailure::Io { operation })?;
    Ok(bytes)
}

/// Signs the whole file. The signature is over the bytes, not a prehash.
pub fn bn_sign_file(seed: &[u8], algorithm: i32, path: &str) -> Result<Vec<u8>, CryptoFailure> {
    let message = read_file(path, SIGN_FILE)?;
    bn_sign(seed, algorithm, &message).map_err(|failure| match failure {
        CryptoFailure::WrongKeyType { .. } => CryptoFailure::WrongKeyType {
            operation: SIGN_FILE,
        },
        CryptoFailure::UnsupportedAlgorithm { .. } => CryptoFailure::UnsupportedAlgorithm {
            operation: SIGN_FILE,
        },
        CryptoFailure::Key(_) => CryptoFailure::Key(SIGN_FILE),
        other => other,
    })
}

/// `FALSE` when the file cannot be read or the signature does not verify.
#[must_use]
pub fn bn_verify_file(raw_public: &[u8], algorithm: i32, path: &str, signature: &[u8]) -> bool {
    let Ok(message) = read_file(path, "BNCrypto.VerifyFile") else {
        return false;
    };
    bn_verify(raw_public, algorithm, &message, signature)
}

fn require_pkcs8(format: i32, operation: &'static str) -> Result<(), CryptoFailure> {
    if format == PEM_FORMAT_PKCS8 {
        Ok(())
    } else {
        Err(CryptoFailure::Format { operation })
    }
}

fn rng() -> pkcs8::rand_core::OsRng {
    pkcs8::rand_core::OsRng
}

fn pem_string(pem: impl AsRef<str>) -> String {
    pem.as_ref().to_owned()
}

fn x25519_private_der(seed: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    let seed: [u8; 32] = seed.try_into().map_err(|_| parse(EXPORT))?;
    let mut wrapped = [0_u8; 34];
    wrapped[0] = 0x04;
    wrapped[1] = 0x20;
    wrapped[2..].copy_from_slice(&seed);
    let algorithm = pkcs8::AlgorithmIdentifierRef {
        oid: pkcs8::ObjectIdentifier::new_unwrap("1.3.101.110"),
        parameters: None,
    };
    let info = pkcs8::PrivateKeyInfo::new(algorithm, &wrapped);
    info.to_der().map_err(|_| parse(EXPORT))
}

fn encrypt_der(der: &[u8], password: &str) -> Result<String, CryptoFailure> {
    let info = pkcs8::PrivateKeyInfo::from_der(der).map_err(|_| parse(EXPORT))?;
    let document = info
        .encrypt(rng(), password.as_bytes())
        .map_err(|_| parse(EXPORT))?;
    document
        .to_pem(LABEL_ENCRYPTED, pkcs8::LineEnding::LF)
        .map(pem_string)
        .map_err(|_| parse(EXPORT))
}

/// PKCS#8 PEM. Encryption is PBES2 scrypt + AES-256-CBC. An empty password still encrypts.
pub fn bn_export_private_pem(
    seed: &[u8],
    algorithm: i32,
    format: i32,
    password: &str,
) -> Result<String, CryptoFailure> {
    require_pkcs8(format, EXPORT)?;
    match algorithm {
        1 | 4 | 30 => {
            let seed: [u8; 32] = seed.try_into().map_err(|_| CryptoFailure::Key(EXPORT))?;
            let key = ed25519_dalek::SigningKey::from_bytes(&seed);
            key.to_pkcs8_encrypted_pem(rng(), password.as_bytes(), pkcs8::LineEnding::LF)
                .map(pem_string)
                .map_err(|_| parse(EXPORT))
        }
        5 | 32 => {
            let key = p256::SecretKey::from_slice(seed).map_err(|_| CryptoFailure::Key(EXPORT))?;
            key.to_pkcs8_encrypted_pem(rng(), password.as_bytes(), pkcs8::LineEnding::LF)
                .map(pem_string)
                .map_err(|_| parse(EXPORT))
        }
        31 => {
            let der = x25519_private_der(seed)?;
            encrypt_der(&der, password)
        }
        _ => Err(CryptoFailure::UnsupportedAlgorithm { operation: EXPORT }),
    }
}

/// Explicit plaintext PKCS#8. This is the only unencrypted private-key export.
pub fn bn_export_private_pem_unencrypted(
    seed: &[u8],
    algorithm: i32,
    format: i32,
) -> Result<String, CryptoFailure> {
    require_pkcs8(format, EXPORT_PLAIN)?;
    match algorithm {
        1 | 4 | 30 => {
            let seed: [u8; 32] = seed
                .try_into()
                .map_err(|_| CryptoFailure::Key(EXPORT_PLAIN))?;
            let key = ed25519_dalek::SigningKey::from_bytes(&seed);
            key.to_pkcs8_pem(pkcs8::LineEnding::LF)
                .map(pem_string)
                .map_err(|_| parse(EXPORT_PLAIN))
        }
        5 | 32 => {
            let key =
                p256::SecretKey::from_slice(seed).map_err(|_| CryptoFailure::Key(EXPORT_PLAIN))?;
            key.to_pkcs8_pem(pkcs8::LineEnding::LF)
                .map(pem_string)
                .map_err(|_| parse(EXPORT_PLAIN))
        }
        31 => {
            let der = x25519_private_der(seed).map_err(|_| parse(EXPORT_PLAIN))?;
            Ok(bn_pem_encode(LABEL_PRIVATE, &der))
        }
        _ => Err(CryptoFailure::UnsupportedAlgorithm {
            operation: EXPORT_PLAIN,
        }),
    }
}

/// SPKI PEM for a public key (`PUBLIC KEY`).
pub fn bn_export_public_pem(spki: &[u8]) -> String {
    bn_pem_encode(LABEL_PUBLIC, spki)
}

fn one_pem_block(pem: &str, operation: &'static str) -> Result<(String, Vec<u8>), CryptoFailure> {
    if pem.matches("-----BEGIN ").count() != 1 {
        return Err(parse(operation));
    }
    let (label, document) = pkcs8::Document::from_pem(pem).map_err(|_| parse(operation))?;
    Ok((label.to_owned(), document.as_bytes().to_vec()))
}

fn x25519_seed_from_private(private_key: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    if private_key.len() == 34 && private_key[0] == 0x04 && private_key[1] == 0x20 {
        return Ok(private_key[2..].to_vec());
    }
    if private_key.len() == 32 {
        return Ok(private_key.to_vec());
    }
    Err(parse(LOAD_PRIVATE))
}

fn material_from_pkcs8(der: &[u8]) -> Result<(Vec<u8>, i32), CryptoFailure> {
    let info = pkcs8::PrivateKeyInfo::from_der(der).map_err(|_| parse(LOAD_PRIVATE))?;
    let ed25519 = pkcs8::ObjectIdentifier::new_unwrap("1.3.101.112");
    let x25519 = pkcs8::ObjectIdentifier::new_unwrap("1.3.101.110");
    let ec = pkcs8::ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
    if info.algorithm.oid == ed25519 {
        let key = ed25519_dalek::SigningKey::try_from(info).map_err(|_| parse(LOAD_PRIVATE))?;
        return Ok((key.to_bytes().to_vec(), 30));
    }
    if info.algorithm.oid == x25519 {
        return Ok((x25519_seed_from_private(info.private_key)?, 31));
    }
    if info.algorithm.oid == ec {
        let key = p256::SecretKey::from_pkcs8_der(der).map_err(|_| parse(LOAD_PRIVATE))?;
        return Ok((key.to_bytes().to_vec(), 32));
    }
    Err(CryptoFailure::UnsupportedAlgorithm {
        operation: LOAD_PRIVATE,
    })
}

/// Ceilings on the key-derivation cost of an encrypted PEM, checked before
/// any derivation: the parameters come from the file, so without them a hostile
/// key aborts the process (scrypt memory is 128 · r · N bytes) or hangs it
/// (PBKDF2 iterations). The scrypt ceilings are the parameters `ExportPem`
/// writes (N = 2^17, r = 8, p = 1, 128 MiB); the PBKDF2 ceiling is far above
/// the OpenSSL default (2048) and the OWASP recommendation for HMAC-SHA-256.
const SCRYPT_MAX_COST: u64 = 1 << 17;
const SCRYPT_MAX_BLOCK_SIZE: u16 = 8;
const SCRYPT_MAX_PARALLELIZATION: u16 = 1;
const PBKDF2_MAX_ITERATIONS: u32 = 2_000_000;

/// Whether the PBES2 key derivation of an encrypted PEM is within the ceilings.
/// PBES1 and unknown schemes are refused by `decrypt` itself.
fn kdf_cost_is_bounded(scheme: &pkcs8::pkcs5::EncryptionScheme<'_>) -> bool {
    use pkcs8::pkcs5::pbes2::Kdf;
    let Some(parameters) = scheme.pbes2() else {
        return true;
    };
    match &parameters.kdf {
        Kdf::Scrypt(scrypt) => {
            scrypt.cost_parameter <= SCRYPT_MAX_COST
                && scrypt.block_size <= SCRYPT_MAX_BLOCK_SIZE
                && scrypt.parallelization <= SCRYPT_MAX_PARALLELIZATION
        }
        Kdf::Pbkdf2(pbkdf2) => pbkdf2.iteration_count <= PBKDF2_MAX_ITERATIONS,
        _ => false,
    }
}

/// Loads one private key. `password == None` is NULL: plaintext PKCS#8 only.
/// A password on an unencrypted key, or more than one PEM block, is `PARSE_ERROR`.
/// A wrong password is `WRONG_PASSWORD` and the password is not copied into the error.
pub fn bn_load_private_pem(
    pem: &str,
    password: Option<&str>,
) -> Result<crate::crypto_asymmetric::PrivateKeyEntry, CryptoFailure> {
    let (label, der) = one_pem_block(pem, LOAD_PRIVATE)?;
    let pkcs8 = if label == LABEL_ENCRYPTED {
        let Some(password) = password else {
            return Err(CryptoFailure::WrongPassword {
                operation: LOAD_PRIVATE,
            });
        };
        let info = pkcs8::EncryptedPrivateKeyInfo::from_der(der.as_slice())
            .map_err(|_| parse(LOAD_PRIVATE))?;
        if !kdf_cost_is_bounded(&info.encryption_algorithm) {
            return Err(parse(LOAD_PRIVATE));
        }
        let secret =
            info.decrypt(password.as_bytes())
                .map_err(|_| CryptoFailure::WrongPassword {
                    operation: LOAD_PRIVATE,
                })?;
        material_from_pkcs8(secret.as_bytes())?
    } else if label == LABEL_PRIVATE {
        if password.is_some() {
            return Err(parse(LOAD_PRIVATE));
        }
        material_from_pkcs8(&der)?
    } else {
        return Err(parse(LOAD_PRIVATE));
    };
    private_entry_from_seed(pkcs8.0, pkcs8.1)
}

/// Loads one SPKI public key. A private PEM is `PARSE_ERROR`, not a guessed public half.
pub fn bn_load_public_pem(
    pem: &str,
) -> Result<crate::crypto_asymmetric::PublicKeyEntry, CryptoFailure> {
    let (label, der) = one_pem_block(pem, LOAD_PUBLIC)?;
    if label != LABEL_PUBLIC {
        return Err(parse(LOAD_PUBLIC));
    }
    public_entry_from_spki(&der)
}

fn buffer(handle: u64) -> Result<Vec<u8>, CryptoFailure> {
    with_buffers(|buffers| buffers.get(&handle).map(|bytes| bytes.to_vec()))
        .ok_or(CryptoFailure::Key(SIGN))
}

#[allow(unsafe_code)]
fn write_handle(out: *mut u64, handle: u64) -> i32 {
    if out.is_null() {
        return -1;
    }
    // SAFETY: `out` was checked non-null and points at a caller-owned slot.
    unsafe { *out = handle };
    0
}

#[allow(unsafe_code)]
fn write_string(out: *mut *mut c_char, text: &str) -> i32 {
    if out.is_null() {
        return -1;
    }
    // SAFETY: `out` was checked non-null and points at a caller-owned slot.
    unsafe { *out = c_string(text) };
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_sign(key: u64, message: u64, out: *mut u64) -> i32 {
    let material = match private_material(key, SIGN) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    let message = match buffer(message) {
        Ok(message) => message,
        Err(failure) => return failed(&failure),
    };
    match bn_sign(&material.seed, material.algorithm, &message) {
        Ok(signature) => write_handle(out, store(signature)),
        Err(failure) => failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_verify(key: u64, message: u64, signature: u64) -> i32 {
    let Some((raw, algorithm)) = public_material(key) else {
        return 0;
    };
    let (Ok(message), Ok(signature)) = (buffer(message), buffer(signature)) else {
        return 0;
    };
    i32::from(bn_verify(&raw, algorithm, &message, &signature))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_sign_file(key: u64, path: *const c_char, out: *mut u64) -> i32 {
    let Some(path) = c_str(path) else {
        return failed(&CryptoFailure::Io {
            operation: SIGN_FILE,
        });
    };
    let material = match private_material(key, SIGN_FILE) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    match bn_sign_file(&material.seed, material.algorithm, path) {
        Ok(signature) => write_handle(out, store(signature)),
        Err(failure) => failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_verify_file(key: u64, path: *const c_char, signature: u64) -> i32 {
    let (Some((raw, algorithm)), Some(path), Ok(signature)) =
        (public_material(key), c_str(path), buffer(signature))
    else {
        return 0;
    };
    i32::from(bn_verify_file(&raw, algorithm, path, &signature))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_export_pem(
    key: u64,
    format: i32,
    password: *const c_char,
    out: *mut *mut c_char,
) -> i32 {
    let Some(password) = c_str(password) else {
        return failed(&CryptoFailure::Format { operation: EXPORT });
    };
    let material = match private_material(key, EXPORT) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    match bn_export_private_pem(&material.seed, material.algorithm, format, password) {
        Ok(pem) => write_string(out, &pem),
        Err(failure) => failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_private_export_pem_plain(
    key: u64,
    format: i32,
    out: *mut *mut c_char,
) -> i32 {
    let material = match private_material(key, EXPORT_PLAIN) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    match bn_export_private_pem_unencrypted(&material.seed, material.algorithm, format) {
        Ok(pem) => write_string(out, &pem),
        Err(failure) => failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_export_pem(key: u64, out: *mut *mut c_char) -> i32 {
    let Some(spki) = with_public_keys(|keys| keys.get(&key).map(|entry| entry.spki.clone())) else {
        return failed(&CryptoFailure::Key("BNCrypto.PublicKey.ExportPem"));
    };
    write_string(out, &bn_export_public_pem(&spki))
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_load_private_key(
    pem: *const c_char,
    password: *const c_char,
    out: *mut u64,
) -> i32 {
    let Some(pem) = c_str(pem) else {
        return failed(&parse(LOAD_PRIVATE));
    };
    let password = c_str(password);
    match bn_load_private_pem(pem, password) {
        Ok(entry) => write_handle(out, store_private_key(entry)),
        Err(failure) => failed(&failure),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_load_public_key(pem: *const c_char, out: *mut u64) -> i32 {
    let Some(pem) = c_str(pem) else {
        return failed(&parse(LOAD_PUBLIC));
    };
    match bn_load_public_pem(pem) {
        Ok(entry) => write_handle(out, store_public_key(entry)),
        Err(failure) => failed(&failure),
    }
}
