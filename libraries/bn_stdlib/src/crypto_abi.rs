// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI handle table for `BNCrypto.Bytes` in compiled binaries. The
//! interpreter keeps its own table in `bn_lib_crypto`; both hand out opaque
//! `u64` handles and neither observes the other. Digest and hex logic is not
//! duplicated here — it comes from [`crate::crypto`].

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use zeroize::Zeroize;

use crate::crypto::{
    Aead, Argon2Params, argon2id, decode_hex, ed25519_public_key, ed25519_sign, ed25519_verify,
    encode_hex, hmac_sha256, hmac_verify, ml_dsa_keypair, ml_dsa_sign, ml_dsa_verify,
    ml_kem_decapsulate, ml_kem_encapsulate, ml_kem_keypair, open, p256_public_key, p256_sign,
    p256_verify, seal,
};
use crate::{c_str, c_string};

/// Returned by fallible entry points; `0` is success, mirroring the other
/// `bn_rt` ABIs.
pub(crate) const BN_CRYPTO_OK: i32 = 0;
pub(crate) const BN_CRYPTO_INVALID_ARGUMENT: i32 = 1;
pub(crate) const BN_CRYPTO_INVALID_HANDLE: i32 = 2;

/// Key entry storing cryptographic material with zeroize-on-drop protection.
#[derive(Debug)]
pub struct KeyEntry {
    pub algorithm: i32,
    pub material: zeroize::Zeroizing<Vec<u8>>,
    pub zeroized: bool,
}

#[derive(Default)]
struct Keys {
    values: Mutex<HashMap<u64, KeyEntry>>,
    holders: Mutex<HashMap<u64, usize>>,
}

fn with_keys<T>(operation: impl FnOnce(&mut HashMap<u64, KeyEntry>) -> T) -> T {
    let keys = bn_core_copy::table::<Keys>();
    operation(
        &mut keys
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn store_key(algorithm: i32, material: zeroize::Zeroizing<Vec<u8>>) -> u64 {
    let handle = next_handle();
    with_keys(|keys| {
        keys.insert(
            handle,
            KeyEntry {
                algorithm,
                material,
                zeroized: false,
            },
        );
    });
    let keys = bn_core_copy::table::<Keys>();
    keys.holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

fn get_key_material(handle: u64) -> Option<zeroize::Zeroizing<Vec<u8>>> {
    with_keys(|keys| {
        keys.get(&handle).and_then(|entry| {
            if entry.zeroized {
                None
            } else {
                Some(entry.material.clone())
            }
        })
    })
    .or_else(|| with_buffers(|b| b.get(&handle).map(|b| zeroize::Zeroizing::new(b.clone()))))
}

/// The byte buffers of the module copy that runs on this thread
/// (`bn_core_copy`): a task's are freed when the task ends.
#[derive(Default)]
struct Buffers {
    values: Mutex<HashMap<u64, Vec<u8>>>,
    holders: Mutex<HashMap<u64, usize>>,
}

fn next_handle() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn with_buffers<T>(operation: impl FnOnce(&mut HashMap<u64, Vec<u8>>) -> T) -> T {
    let buffers = bn_core_copy::table::<Buffers>();
    operation(
        &mut buffers
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn store(bytes: Vec<u8>) -> u64 {
    let handle = next_handle();
    with_buffers(|buffers| buffers.insert(handle, bytes));
    let buffers = bn_core_copy::table::<Buffers>();
    buffers
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

/// Stores the UTF-8 bytes of `text` and returns the handle. A null or
/// non-UTF-8 argument yields an empty buffer rather than a null handle, so the
/// caller never has to branch on a sentinel.
#[allow(unsafe_code)] // C ABI: STRING in, opaque handle out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_from_text(text: *const c_char) -> u64 {
    store(
        c_str(text)
            .map(|text| text.as_bytes().to_vec())
            .unwrap_or_default(),
    )
}

/// Decodes `text` as hex into a new buffer. Writes the handle through `out` and
/// returns [`BN_CRYPTO_OK`], or leaves `out` untouched and returns
/// [`BN_CRYPTO_INVALID_ARGUMENT`] when the string is not even-length hex.
#[allow(unsafe_code)] // C ABI: STRING in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_from_hex(text: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let text = c_str(text).unwrap_or_default();
    let Some(bytes) = decode_hex(text) else {
        return failed(&crate::crypto_error::CryptoFailure::Hex(text.to_owned()));
    };
    let handle = store(bytes);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Byte length of `handle`, or `-1` when the handle is unknown.
#[allow(unsafe_code)] // C ABI: opaque handle in, length out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_length(handle: u64) -> i64 {
    with_buffers(|buffers| {
        buffers
            .get(&handle)
            .and_then(|bytes| i64::try_from(bytes.len()).ok())
            .unwrap_or(-1)
    })
}

/// Lowercase hex of `handle` as a freshly allocated NUL-terminated string,
/// owned by the caller. An unknown handle yields `""`.
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_to_hex(handle: u64) -> *mut c_char {
    let hex = with_buffers(|buffers| buffers.get(&handle).map(|bytes| encode_hex(bytes)));
    c_string(&hex.unwrap_or_default())
}

/// One more binding holds `handle` (a copy). Handle 0 is the error case
/// of `Bytes OR Error`, which holds no buffer.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_retain(handle: u64) -> i32 {
    if handle == 0 {
        return BN_CRYPTO_OK;
    }
    let buffers = bn_core_copy::table::<Buffers>();
    let mut holders = buffers
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

/// Drops one reference to `handle`. When the count reaches zero, the buffer
/// is freed. Returns [`BN_CRYPTO_INVALID_HANDLE`] when it was already released.
/// Handle 0 is the error case of `Bytes OR Error`.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_release(handle: u64) -> i32 {
    if handle == 0 {
        return BN_CRYPTO_OK;
    }
    let buffers = bn_core_copy::table::<Buffers>();
    let mut holders = buffers
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
        with_buffers(|buffers| buffers.remove(&handle));
    }
    BN_CRYPTO_OK
}

/// Algorithm selector shared with the backends: `0` is AES-256-GCM, `1` is
/// ChaCha20-Poly1305. Anything else is rejected.
/// Records `failure` for the emitted code's `Error` and returns the status.
fn failed(failure: &crate::crypto_error::CryptoFailure) -> i32 {
    crate::set_error_report(
        failure.code(),
        failure.operation(),
        failure.message(),
        failure.cause(),
    );
    BN_CRYPTO_INVALID_ARGUMENT
}

fn algorithm(selector: i32) -> Option<Aead> {
    match selector {
        0 => Some(Aead::Aes256Gcm),
        1 => Some(Aead::ChaCha20Poly1305),
        _ => None,
    }
}

/// The four buffers an AEAD call needs: key, nonce, message and AAD.
type AeadOperands = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

fn borrow_aead(key_handle: u64, b: u64, c: u64, d: u64) -> Option<AeadOperands> {
    let key = get_key_material(key_handle)?;
    let (nonce, msg, aad) = with_buffers(|buffers| {
        Some((
            buffers.get(&b)?.clone(),
            buffers.get(&c)?.clone(),
            buffers.get(&d)?.clone(),
        ))
    })?;
    Some((key.to_vec(), nonce, msg, aad))
}

/// Seals `plaintext` under `key`/`nonce` with `aad`, storing the ciphertext and
/// its tag in a new buffer whose handle is written through `out`.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_seal(
    selector: i32,
    key: u64,
    nonce: u64,
    plaintext: u64,
    aad: u64,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(algorithm) = algorithm(selector) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some((key, nonce, plaintext, aad)) = borrow_aead(key, nonce, plaintext, aad) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(sealed) = seal(algorithm, &key, &nonce, &plaintext, &aad) else {
        return failed(&crate::crypto_error::CryptoFailure::Seal(algorithm));
    };
    let handle = store(sealed);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Verifies and opens `ciphertext`. A tag that does not verify returns
/// [`BN_CRYPTO_INVALID_ARGUMENT`] and writes nothing: there is no partial or
/// unauthenticated plaintext.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_open(
    selector: i32,
    key: u64,
    nonce: u64,
    ciphertext: u64,
    aad: u64,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(algorithm) = algorithm(selector) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some((key, nonce, ciphertext, aad)) = borrow_aead(key, nonce, ciphertext, aad) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(plain) = open(algorithm, &key, &nonce, &ciphertext, &aad) else {
        return failed(&crate::crypto_error::CryptoFailure::Open(algorithm));
    };
    let handle = store(plain);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// HMAC-SHA-256 over two buffers, storing the tag in a new buffer.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hmac(key: u64, data: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(key) = get_key_material(key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(data) = with_buffers(|buffers| buffers.get(&data).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let handle = store(hmac_sha256(&key, &data));
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Constant-time MAC verification. Returns `1` for a match, `0` otherwise, and
/// [`BN_CRYPTO_INVALID_HANDLE`] when a handle is unknown.
#[unsafe(no_mangle)]
#[allow(unsafe_code)] // C ABI: opaque handles in, boolean out.
pub extern "C" fn bn_rt_crypto_hmac_verify(key: u64, data: u64, tag: u64) -> i32 {
    let Some(key) = get_key_material(key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some((data, tag)) = with_buffers(|buffers| {
        Some((
            buffers.get(&data)?.clone(),
            buffers.get(&tag)?.clone(),
        ))
    }) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    i32::from(hmac_verify(&key, &data, &tag))
}

/// Argon2id over a password and salt with the three cost parameters, producing
/// a 32-byte tag. Parameters outside the algorithm's range are rejected rather
/// than clamped to something weaker.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_argon2id(
    password: u64,
    salt: u64,
    memory_kib: i64,
    iterations: i64,
    parallelism: i64,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let (Ok(memory_kib), Ok(iterations), Ok(parallelism)) = (
        u32::try_from(memory_kib),
        u32::try_from(iterations),
        u32::try_from(parallelism),
    ) else {
        return failed(&crate::crypto_error::CryptoFailure::Argon2id);
    };
    let Some((password, salt)) = with_buffers(|buffers| {
        Some((buffers.get(&password)?.clone(), buffers.get(&salt)?.clone()))
    }) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(tag) = argon2id(
        &password,
        &salt,
        Argon2Params {
            secret: &[],
            associated_data: &[],
            memory_kib,
            iterations,
            parallelism,
            tag_length: 32,
        },
    ) else {
        return failed(&crate::crypto_error::CryptoFailure::Argon2id);
    };
    let handle = store(tag);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Signature scheme selector: `0` is Ed25519, `1` is ECDSA P-256.
fn scheme(selector: i32) -> Option<bool> {
    match selector {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

/// Public key for a private key handle, under the selected scheme.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_public_key(selector: i32, private_key: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(ed25519) = scheme(selector) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some(private_key) = get_key_material(private_key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let derived = if ed25519 {
        ed25519_public_key(&private_key)
    } else {
        p256_public_key(&private_key)
    };
    let Some(public_key) = derived else {
        return failed(&crate::crypto_error::CryptoFailure::Key(if ed25519 {
            "BNCrypto.Ed25519PublicKey"
        } else {
            "BNCrypto.EcdsaP256PublicKey"
        }));
    };
    let handle = store(public_key);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Signs `message` under the selected scheme. Both schemes are deterministic.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_sign(
    selector: i32,
    private_key: u64,
    message: u64,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(ed25519) = scheme(selector) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some(private_key) = get_key_material(private_key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(message) = with_buffers(|buffers| buffers.get(&message).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let signed = if ed25519 {
        ed25519_sign(&private_key, &message)
    } else {
        p256_sign(&private_key, &message)
    };
    let Some(signature) = signed else {
        return failed(&crate::crypto_error::CryptoFailure::Key(if ed25519 {
            "BNCrypto.Ed25519Sign"
        } else {
            "BNCrypto.EcdsaP256Sign"
        }));
    };
    let handle = store(signature);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Verifies a signature. Returns `1` for a valid signature and `0` for anything
/// else — a malformed key, a tampered message, or a signature from another key.
#[allow(unsafe_code)] // C ABI: opaque handles in, boolean out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_verify(
    selector: i32,
    public_key: u64,
    message: u64,
    signature: u64,
) -> i32 {
    let Some(ed25519) = scheme(selector) else {
        return 0;
    };
    let Some((public_key, message, signature)) = with_buffers(|buffers| {
        Some((
            buffers.get(&public_key)?.clone(),
            buffers.get(&message)?.clone(),
            buffers.get(&signature)?.clone(),
        ))
    }) else {
        return 0;
    };
    i32::from(if ed25519 {
        ed25519_verify(&public_key, &message, &signature)
    } else {
        p256_verify(&public_key, &message, &signature)
    })
}

/// ML-KEM-768 key pair from a 64-byte seed handle, as `publicKey || seed`.
#[allow(unsafe_code)] // C ABI: opaque handle in, two opaque handles out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_kem_keypair(seed: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(seed) = get_key_material(seed) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some((mut public_key, private_key)) = ml_kem_keypair(&seed) else {
        return failed(&crate::crypto_error::CryptoFailure::Seed(
            "BNCrypto.MlKemKeypair",
        ));
    };
    public_key.extend_from_slice(&private_key);
    let handle = store(public_key);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Encapsulates with fresh randomness, as `ciphertext || sharedSecret`.
#[allow(unsafe_code)] // C ABI: opaque handle in, two opaque handles out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_kem_encapsulate(public_key: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(public_key) = with_buffers(|buffers| buffers.get(&public_key).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some((mut ciphertext, secret)) = ml_kem_encapsulate(&public_key) else {
        return failed(&crate::crypto_error::CryptoFailure::EncapsulationKey);
    };
    ciphertext.extend_from_slice(&secret);
    let handle = store(ciphertext);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Recovers the shared secret from a ciphertext.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_kem_decapsulate(
    private_key: u64,
    ciphertext: u64,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(private_key) = get_key_material(private_key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(ciphertext) = with_buffers(|buffers| buffers.get(&ciphertext).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(secret) = ml_kem_decapsulate(&private_key, &ciphertext) else {
        return failed(&crate::crypto_error::CryptoFailure::Decapsulate);
    };
    let handle = store(secret);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// ML-DSA-65 key pair from a 32-byte seed handle, as `verifyingKey || seed`.
#[allow(unsafe_code)] // C ABI: opaque handle in, two opaque handles out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_dsa_keypair(seed: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(seed) = get_key_material(seed) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some((mut public_key, private_key)) = ml_dsa_keypair(&seed) else {
        return failed(&crate::crypto_error::CryptoFailure::Seed(
            "BNCrypto.MlDsaKeypair",
        ));
    };
    public_key.extend_from_slice(&private_key);
    let handle = store(public_key);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// ML-DSA-65 signature.
#[allow(unsafe_code)] // C ABI: opaque handles in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_dsa_sign(private_key: u64, message: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(private_key) = get_key_material(private_key) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(message) = with_buffers(|buffers| buffers.get(&message).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(signature) = ml_dsa_sign(&private_key, &message) else {
        return failed(&crate::crypto_error::CryptoFailure::SigningKey);
    };
    let handle = store(signature);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Verifies an ML-DSA-65 signature. `1` for valid, `0` for anything else.
#[allow(unsafe_code)] // C ABI: opaque handles in, boolean out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_dsa_verify(public_key: u64, message: u64, signature: u64) -> i32 {
    let Some((public_key, message, signature)) = with_buffers(|buffers| {
        Some((
            buffers.get(&public_key)?.clone(),
            buffers.get(&message)?.clone(),
            buffers.get(&signature)?.clone(),
        ))
    }) else {
        return 0;
    };
    i32::from(ml_dsa_verify(&public_key, &message, &signature))
}

/// A sub-range of a buffer. Out-of-range is rejected rather than clamped, so a
/// bad offset can never yield a short buffer that looks like a key.
#[allow(unsafe_code)] // C ABI: opaque handle in, opaque handle out through `out`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_slice(data: u64, start: i64, length: i64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(data) = with_buffers(|buffers| buffers.get(&data).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let range = usize::try_from(start)
        .ok()
        .zip(usize::try_from(length).ok())
        .and_then(|(from, count)| Some((from, from.checked_add(count)?)))
        .filter(|(_, end)| *end <= data.len());
    let Some((start, end)) = range else {
        return failed(&crate::crypto_error::CryptoFailure::Slice {
            start,
            length,
            size: data.len(),
        });
    };
    let handle = store(data[start..end].to_vec());
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Generates `length` random bytes using the operating system CSPRNG and stores them in a new buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_bytes(length: i64, out: *mut u64) -> i32 {
    if out.is_null() || length < 0 {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Ok(len) = usize::try_from(length) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let bytes = match crate::crypto::random_bytes(len) {
        Ok(b) => b,
        Err(err) => return failed(&err),
    };
    let handle = store(bytes);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Generates a CSPRNG nonce suitable for `algorithm` and stores it in a new buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_generate_nonce(algorithm: i32, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let nonce = match crate::crypto::generate_nonce(algorithm) {
        Ok(n) => n,
        Err(err) => return failed(&err),
    };
    let handle = store(nonce);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Generates a new cryptographic key for `algorithm` using OS entropy.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_generate_key(algorithm: i32, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let material = match crate::crypto::generate_key_material(algorithm) {
        Ok(m) => m,
        Err(err) => return failed(&err),
    };
    let handle = store_key(algorithm, material);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Imports raw key material from `material_handle` as a `Key` for `algorithm`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_import_key(
    material_handle: u64,
    algorithm: i32,
    out: *mut u64,
) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(expected_len) = crate::crypto::required_key_length(algorithm) else {
        return failed(&crate::crypto_error::CryptoFailure::Algorithm(algorithm));
    };
    let Some(raw_bytes) = with_buffers(|b| b.get(&material_handle).cloned()) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    if raw_bytes.len() != expected_len {
        return failed(&crate::crypto_error::CryptoFailure::KeyLength {
            expected: expected_len,
            actual: raw_bytes.len(),
        });
    }
    let material = zeroize::Zeroizing::new(raw_bytes);
    let handle = store_key(algorithm, material);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Returns the byte length of `key_handle`, or `-1` if unknown/zeroized.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_key_length(key_handle: u64) -> i64 {
    with_keys(|keys| {
        keys.get(&key_handle)
            .and_then(|entry| {
                if entry.zeroized {
                    None
                } else {
                    i64::try_from(entry.material.len()).ok()
                }
            })
            .unwrap_or(-1)
    })
}

/// Returns the algorithm constant of `key_handle`, or `-1` if unknown/zeroized.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_key_algorithm(key_handle: u64) -> i32 {
    with_keys(|keys| {
        keys.get(&key_handle)
            .and_then(|entry| if entry.zeroized { None } else { Some(entry.algorithm) })
            .unwrap_or(-1)
    })
}

/// Explicitly zeroizes the key material in `key_handle`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_key_zeroize(key_handle: u64) -> i32 {
    with_keys(|keys| {
        if let Some(entry) = keys.get_mut(&key_handle) {
            entry.material.zeroize();
            entry.zeroized = true;
            BN_CRYPTO_OK
        } else {
            BN_CRYPTO_INVALID_HANDLE
        }
    })
}

/// Retains a reference to `key_handle`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_key_retain(key_handle: u64) -> i32 {
    if key_handle == 0 {
        return BN_CRYPTO_OK;
    }
    let keys = bn_core_copy::table::<Keys>();
    let mut holders = keys
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&key_handle) {
        *count += 1;
        BN_CRYPTO_OK
    } else {
        BN_CRYPTO_INVALID_HANDLE
    }
}

/// Releases a reference to `key_handle`. When refcount drops to 0, zeroizes and frees the key.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_key_release(key_handle: u64) -> i32 {
    if key_handle == 0 {
        return BN_CRYPTO_OK;
    }
    let keys = bn_core_copy::table::<Keys>();
    let mut holders = keys
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(count) = holders.get_mut(&key_handle) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&key_handle);
        drop(holders);
        with_keys(|keys| {
            if let Some(mut entry) = keys.remove(&key_handle) {
                entry.material.zeroize();
            }
        });
    }
    BN_CRYPTO_OK
}

#[cfg(test)]
mod tests {
    use super::{
        BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_INVALID_HANDLE, BN_CRYPTO_OK,
        bn_rt_crypto_bytes_from_hex, bn_rt_crypto_bytes_from_text, bn_rt_crypto_bytes_length,
        bn_rt_crypto_bytes_release, bn_rt_crypto_slice, store,
    };

    #[test]
    fn handles_are_distinct_and_release_once() {
        let first = store(vec![1, 2, 3]);
        let second = store(vec![4, 5]);
        assert_ne!(first, second);
        assert_eq!(bn_rt_crypto_bytes_length(first), 3);
        assert_eq!(bn_rt_crypto_bytes_length(second), 2);
        assert_eq!(bn_rt_crypto_bytes_release(first), BN_CRYPTO_OK);
        assert_eq!(bn_rt_crypto_bytes_release(first), BN_CRYPTO_INVALID_HANDLE);
        assert_eq!(bn_rt_crypto_bytes_length(first), -1);
        assert_eq!(bn_rt_crypto_bytes_release(second), BN_CRYPTO_OK);
    }

    #[test]
    fn crypto_abi_rejects_null_invalid_utf8_and_bva_slice_overflow() {
        let mut out = 0u64;
        assert_eq!(
            bn_rt_crypto_bytes_from_hex(std::ptr::null(), std::ptr::null_mut()),
            BN_CRYPTO_INVALID_ARGUMENT
        );
        let bad_utf8 = [0xFFu8, 0x00u8];
        let empty_h = bn_rt_crypto_bytes_from_text(bad_utf8.as_ptr().cast());
        assert_eq!(bn_rt_crypto_bytes_length(empty_h), 0);

        let buf = store(vec![10, 20, 30, 40]);
        for (start, len) in [(-1i64, 2i64), (0, -1), (2, i64::MAX), (i64::MAX, 1), (0, 5)] {
            assert_ne!(
                bn_rt_crypto_slice(buf, start, len, &raw mut out),
                BN_CRYPTO_OK,
                "slice({start}, {len}) must fail"
            );
        }
        assert_eq!(
            bn_rt_crypto_slice(buf, 1, 2, std::ptr::null_mut()),
            BN_CRYPTO_INVALID_ARGUMENT
        );

        assert_eq!(bn_rt_crypto_bytes_release(empty_h), BN_CRYPTO_OK);
        assert_eq!(bn_rt_crypto_bytes_release(buf), BN_CRYPTO_OK);
    }
}
