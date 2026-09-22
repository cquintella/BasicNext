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
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::crypto::{Aead, decode_hex, encode_hex, open, seal};
use crate::{c_str, c_string};

/// Returned by fallible entry points; `0` is success, mirroring the other
/// `bn_rt` ABIs.
pub const BN_CRYPTO_OK: i32 = 0;
pub const BN_CRYPTO_INVALID_ARGUMENT: i32 = 1;
pub const BN_CRYPTO_INVALID_HANDLE: i32 = 2;

fn buffers() -> &'static Mutex<HashMap<u64, Vec<u8>>> {
    static BUFFERS: OnceLock<Mutex<HashMap<u64, Vec<u8>>>> = OnceLock::new();
    BUFFERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_handle() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn with_buffers<T>(operation: impl FnOnce(&mut HashMap<u64, Vec<u8>>) -> T) -> T {
    operation(&mut buffers().lock().unwrap_or_else(PoisonError::into_inner))
}

fn store(bytes: Vec<u8>) -> u64 {
    let handle = next_handle();
    with_buffers(|buffers| buffers.insert(handle, bytes));
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
    let Some(bytes) = c_str(text).and_then(decode_hex) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let handle = store(bytes);
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

/// Drops `handle`. Returns [`BN_CRYPTO_INVALID_HANDLE`] when it was already
/// released, so a double `RELEASE` is reported rather than ignored.
#[allow(unsafe_code)] // C ABI: opaque handle in, status out.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_release(handle: u64) -> i32 {
    with_buffers(|buffers| {
        if buffers.remove(&handle).is_some() {
            BN_CRYPTO_OK
        } else {
            BN_CRYPTO_INVALID_HANDLE
        }
    })
}

/// Algorithm selector shared with the backends: `0` is AES-256-GCM, `1` is
/// ChaCha20-Poly1305. Anything else is rejected.
fn algorithm(selector: i32) -> Option<Aead> {
    match selector {
        0 => Some(Aead::Aes256Gcm),
        1 => Some(Aead::ChaCha20Poly1305),
        _ => None,
    }
}

/// The four buffers an AEAD call needs: key, nonce, message and AAD.
type AeadOperands = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

fn borrow4(a: u64, b: u64, c: u64, d: u64) -> Option<AeadOperands> {
    with_buffers(|buffers| {
        Some((
            buffers.get(&a)?.clone(),
            buffers.get(&b)?.clone(),
            buffers.get(&c)?.clone(),
            buffers.get(&d)?.clone(),
        ))
    })
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
    let Some((key, nonce, plaintext, aad)) = borrow4(key, nonce, plaintext, aad) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(sealed) = seal(algorithm, &key, &nonce, &plaintext, &aad) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let handle = store(sealed);
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
    let Some((key, nonce, ciphertext, aad)) = borrow4(key, nonce, ciphertext, aad) else {
        return BN_CRYPTO_INVALID_HANDLE;
    };
    let Some(plain) = open(algorithm, &key, &nonce, &ciphertext, &aad) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let handle = store(plain);
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

#[cfg(test)]
mod tests {
    use super::{
        BN_CRYPTO_INVALID_HANDLE, BN_CRYPTO_OK, bn_rt_crypto_bytes_length,
        bn_rt_crypto_bytes_release, store,
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
    }
}
