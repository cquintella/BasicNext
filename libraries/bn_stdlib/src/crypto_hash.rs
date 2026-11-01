// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Cryptographic hash functions and streaming (`language/0.6/bncrypto.md`).
//! Provides SHA-2, SHA-3, `BLAKE2b`, and `BLAKE3`, streaming `Hasher`, and file hashing.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use blake2::Blake2b512;
use sha2::Digest;
use sha2::{Sha256, Sha384, Sha512, Sha512_256};
use sha3::{Sha3_256, Sha3_512};

use crate::crypto_error::CryptoFailure;

pub const HASH_SHA256: i32 = 1;
pub const HASH_SHA384: i32 = 2;
pub const HASH_SHA512: i32 = 3;
pub const HASH_SHA512_256: i32 = 4;
pub const HASH_SHA3_256: i32 = 5;
pub const HASH_SHA3_512: i32 = 6;
pub const HASH_BLAKE2B: i32 = 7;
pub const HASH_BLAKE3: i32 = 8;
pub const HASH_MD5: i32 = 9;
pub const HASH_SHA1: i32 = 10;

/// Standard algorithm IDs catalogued for `BNCrypto.Alg`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Alg;

impl Alg {
    pub const SHA256: i32 = HASH_SHA256;
    pub const SHA384: i32 = HASH_SHA384;
    pub const SHA512: i32 = HASH_SHA512;
    pub const SHA512_256: i32 = HASH_SHA512_256;
    pub const SHA3_256: i32 = HASH_SHA3_256;
    pub const SHA3_512: i32 = HASH_SHA3_512;
    pub const BLAKE2B: i32 = HASH_BLAKE2B;
    pub const BLAKE3: i32 = HASH_BLAKE3;
    pub const MD5: i32 = HASH_MD5;
    pub const SHA1: i32 = HASH_SHA1;
    pub const HMAC_SHA256: i32 = crate::crypto_mac::MAC_HMAC_SHA256;
    pub const HMAC_SHA384: i32 = crate::crypto_mac::MAC_HMAC_SHA384;
    pub const HMAC_SHA512: i32 = crate::crypto_mac::MAC_HMAC_SHA512;
    pub const BLAKE3_KEYED: i32 = crate::crypto_mac::MAC_BLAKE3_KEYED;
}

enum State {
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Sha512_256(Sha512_256),
    Sha3_256(Sha3_256),
    Sha3_512(Sha3_512),
    Blake2b(Blake2b512),
    Blake3(Box<blake3::Hasher>),
}

/// Incremental cryptographic hash state.
pub struct Hasher {
    state: State,
}

impl std::fmt::Debug for Hasher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hasher").finish_non_exhaustive()
    }
}

impl Hasher {
    /// Creates a new `Hasher` for the given algorithm ID.
    ///
    /// # Errors
    /// Returns `CryptoFailure::Algorithm` if the algorithm is legacy (MD5/SHA1)
    /// without legacy policy enabled, or unrecognized.
    pub fn new(alg: i32) -> Result<Self, CryptoFailure> {
        let state = match alg {
            HASH_SHA256 => State::Sha256(Sha256::new()),
            HASH_SHA384 => State::Sha384(Sha384::new()),
            HASH_SHA512 => State::Sha512(Sha512::new()),
            HASH_SHA512_256 => State::Sha512_256(Sha512_256::new()),
            HASH_SHA3_256 => State::Sha3_256(Sha3_256::new()),
            HASH_SHA3_512 => State::Sha3_512(Sha3_512::new()),
            HASH_BLAKE2B => State::Blake2b(Blake2b512::new()),
            HASH_BLAKE3 => State::Blake3(Box::new(blake3::Hasher::new())),
            _ => return Err(CryptoFailure::Algorithm(alg)),
        };
        Ok(Self { state })
    }

    /// Feeds input bytes into the running hash calculation.
    pub fn update(&mut self, data: &[u8]) {
        match &mut self.state {
            State::Sha256(h) => h.update(data),
            State::Sha384(h) => h.update(data),
            State::Sha512(h) => h.update(data),
            State::Sha512_256(h) => h.update(data),
            State::Sha3_256(h) => h.update(data),
            State::Sha3_512(h) => h.update(data),
            State::Blake2b(h) => h.update(data),
            State::Blake3(h) => {
                let _ = h.update(data);
            }
        }
    }

    /// Finalizes the hash and consumes the hasher, returning the raw digest bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        match self.state {
            State::Sha256(h) => h.finalize().to_vec(),
            State::Sha384(h) => h.finalize().to_vec(),
            State::Sha512(h) => h.finalize().to_vec(),
            State::Sha512_256(h) => h.finalize().to_vec(),
            State::Sha3_256(h) => h.finalize().to_vec(),
            State::Sha3_512(h) => h.finalize().to_vec(),
            State::Blake2b(h) => h.finalize().to_vec(),
            State::Blake3(h) => h.finalize().as_bytes().to_vec(),
        }
    }
}

/// Computes the one-shot hash of `data` using `alg`.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if `alg` is invalid or unsupported.
pub fn bn_hash(data: &[u8], alg: i32) -> Result<Vec<u8>, CryptoFailure> {
    let mut hasher = Hasher::new(alg)?;
    hasher.update(data);
    Ok(hasher.finish())
}

/// Streams and hashes the contents of `path` in 64 KiB segments.
///
/// # Errors
/// Returns `CryptoFailure::FileIo` on file access error, or `CryptoFailure::Algorithm`.
pub fn bn_hash_file(path: &Path, alg: i32) -> Result<Vec<u8>, CryptoFailure> {
    let mut hasher = Hasher::new(alg)?;
    let mut file = crate::policy::open_path(path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read_bytes = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(CryptoFailure::FileIo(e.to_string())),
        };
        hasher.update(&buffer[..read_bytes]);
    }
    Ok(hasher.finish())
}

static NEXT_HASHER_HANDLE: AtomicI64 = AtomicI64::new(1);
static HASHERS: Mutex<Option<HashMap<i64, Hasher>>> = Mutex::new(None);

fn with_hashers<R>(f: impl FnOnce(&mut HashMap<i64, Hasher>) -> R) -> R {
    let mut guard = HASHERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

/// Inserts a `Hasher` into the global runtime table, returning a handle.
pub fn insert_hasher(hasher: Hasher) -> i64 {
    let handle = NEXT_HASHER_HANDLE.fetch_add(1, Ordering::Relaxed);
    with_hashers(|t| t.insert(handle, hasher));
    handle
}

/// Updates an existing `Hasher` identified by `handle`.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn update_hasher(handle: i64, data: &[u8]) -> Result<(), CryptoFailure> {
    with_hashers(|t| match t.get_mut(&handle) {
        Some(h) => {
            h.update(data);
            Ok(())
        }
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Finalizes and removes a `Hasher` identified by `handle`, returning its digest.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn finish_hasher(handle: i64) -> Result<Vec<u8>, CryptoFailure> {
    with_hashers(|t| match t.remove(&handle) {
        Some(h) => Ok(h.finish()),
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Releases a `Hasher` handle without computing its digest.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle was not found.
pub fn release_hasher(handle: i64) -> Result<(), CryptoFailure> {
    with_hashers(|t| match t.remove(&handle) {
        Some(_) => Ok(()),
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

// ---------------------------------------------------------------------------
// C ABI exports for hashes and streaming Hasher
// ---------------------------------------------------------------------------

use std::ffi::c_char;

use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed, store};
use crate::{c_str, c_string};

/// C ABI: `Hash(data, alg)`. Computes one-shot hash and writes `Bytes` handle to `*out_handle`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash(
    data_ptr: *const u8,
    data_len: usize,
    alg: i32,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() || (data_ptr.is_null() && data_len > 0) {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let data = if data_len == 0 {
        &[][..]
    } else {
        // SAFETY: pointer checked non-null and valid for data_len bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, data_len) }
    };
    match bn_hash(data, alg) {
        Ok(digest) => {
            let handle = store(digest);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Hash(data)` handle helper returning `Bytes` handle directly.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_handle(data_handle: i64, alg: i32) -> i64 {
    let Ok(u_handle) = u64::try_from(data_handle) else {
        return 0;
    };
    let data = crate::crypto_abi::with_buffers(|b| b.get(&u_handle).cloned());
    let Some(bytes) = data else {
        return 0;
    };
    match bn_hash(&bytes, alg) {
        Ok(digest) => i64::try_from(store(digest)).unwrap_or(0),
        Err(err) => {
            failed(&err);
            0
        }
    }
}

/// C ABI: `HashWith(data, opts)` handle helper returning status and writing handle to `*out_handle`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_handle_status(
    data_handle: i64,
    alg: i32,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Ok(u_handle) = u64::try_from(data_handle) else {
        return failed(&CryptoFailure::Algorithm(alg));
    };
    let data = crate::crypto_abi::with_buffers(|b| b.get(&u_handle).cloned());
    let Some(bytes) = data else {
        return failed(&CryptoFailure::Algorithm(alg));
    };
    match bn_hash(&bytes, alg) {
        Ok(digest) => {
            let handle = store(digest);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

const HASH_OPTS_ALG_OFFSET: usize = 16;

/// C ABI: `HashWith(data, opts)` reading `alg` from `HashOptions` struct pointer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_with(
    data_handle: i64,
    opts_ptr: *const u8,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    if opts_ptr.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let alg = unsafe {
        let mut alg_bytes = [0u8; 4];
        std::ptr::copy_nonoverlapping(
            opts_ptr.add(HASH_OPTS_ALG_OFFSET),
            alg_bytes.as_mut_ptr(),
            4,
        );
        i32::from_ne_bytes(alg_bytes)
    };
    bn_rt_crypto_hash_handle_status(data_handle, alg, out_handle)
}

/// C ABI: `HashText(text)`. Computes default SHA-256 hex string and returns STRING.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_text(text: *const c_char) -> *mut c_char {
    let Some(input_str) = c_str(text) else {
        return c_string("");
    };
    let hex = crate::crypto::sha256_hex(input_str.as_bytes());
    c_string(&hex)
}

/// C ABI: `HashFile(path, alg)`. Streams file and writes `Bytes` handle to `*out_handle`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hash_file(
    path_ptr: *const c_char,
    alg: i32,
    out_handle: *mut i64,
) -> i32 {
    if path_ptr.is_null() || out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(path_str) = c_str(path_ptr) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match bn_hash_file(Path::new(path_str), alg) {
        Ok(digest) => {
            let handle = store(digest);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Hasher.New(alg)`. Creates a new Hasher handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hasher_new(alg: i32, out_handle: *mut i64) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    match Hasher::new(alg) {
        Ok(hasher) => {
            let handle = insert_hasher(hasher);
            unsafe { *out_handle = handle };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `hasher.Update(data)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hasher_update(
    hasher_handle: i64,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    if data_ptr.is_null() && data_len > 0 {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let data = if data_len == 0 {
        &[][..]
    } else {
        // SAFETY: pointer checked non-null and valid for data_len bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, data_len) }
    };
    match update_hasher(hasher_handle, data) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}

/// C ABI: `hasher.Update(data)` using `Bytes` handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hasher_update_handle(hasher_handle: i64, data_handle: i64) -> i32 {
    let Ok(u_handle) = u64::try_from(data_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let data = crate::crypto_abi::with_buffers(|b| b.get(&u_handle).cloned());
    let Some(bytes) = data else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match update_hasher(hasher_handle, &bytes) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}

/// C ABI: `hasher.Finish()`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hasher_finish(hasher_handle: i64, out_handle: *mut i64) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    match finish_hasher(hasher_handle) {
        Ok(digest) => {
            let handle = store(digest);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: releases a hasher handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_hasher_release(hasher_handle: i64) -> i32 {
    let _ = release_hasher(hasher_handle);
    BN_CRYPTO_OK
}
