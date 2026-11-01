// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Typed MAC algorithms and streaming (`language/0.6/bncrypto.md`).
//! Provides HMAC-SHA-256, HMAC-SHA-384, HMAC-SHA-512, and `BLAKE3_KEYED`,
//! constant-time verification, and streaming `MacStream`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use hmac::{Mac, SimpleHmac};
use sha2::{Sha256, Sha384, Sha512};
use subtle::ConstantTimeEq;

use crate::crypto_error::CryptoFailure;

pub const MAC_HMAC_SHA256: i32 = 11;
pub const MAC_HMAC_SHA384: i32 = 12;
pub const MAC_HMAC_SHA512: i32 = 13;
pub const MAC_BLAKE3_KEYED: i32 = 14;

enum MacState {
    HmacSha256(SimpleHmac<Sha256>),
    HmacSha384(SimpleHmac<Sha384>),
    HmacSha512(SimpleHmac<Sha512>),
    Blake3(Box<blake3::Hasher>),
}

/// Incremental Message Authentication Code (MAC) state.
pub struct MacStream {
    state: MacState,
}

impl std::fmt::Debug for MacStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacStream").finish_non_exhaustive()
    }
}

impl MacStream {
    /// Creates a new `MacStream` for the given key and algorithm.
    ///
    /// # Errors
    /// Returns `CryptoFailure::Algorithm` if `alg` is unrecognized,
    /// or `CryptoFailure::Key` if the key is invalid for the algorithm.
    pub fn new(key: &[u8], alg: i32) -> Result<Self, CryptoFailure> {
        let state = match alg {
            MAC_HMAC_SHA256 => {
                let mac = SimpleHmac::<Sha256>::new_from_slice(key)
                    .map_err(|_| CryptoFailure::Key("invalid key for HMAC-SHA256"))?;
                MacState::HmacSha256(mac)
            }
            MAC_HMAC_SHA384 => {
                let mac = SimpleHmac::<Sha384>::new_from_slice(key)
                    .map_err(|_| CryptoFailure::Key("invalid key for HMAC-SHA384"))?;
                MacState::HmacSha384(mac)
            }
            MAC_HMAC_SHA512 => {
                let mac = SimpleHmac::<Sha512>::new_from_slice(key)
                    .map_err(|_| CryptoFailure::Key("invalid key for HMAC-SHA512"))?;
                MacState::HmacSha512(mac)
            }
            MAC_BLAKE3_KEYED => {
                let key_array: &[u8; 32] = key
                    .try_into()
                    .map_err(|_| CryptoFailure::Key("BLAKE3 keyed MAC requires a 32-byte key"))?;
                MacState::Blake3(Box::new(blake3::Hasher::new_keyed(key_array)))
            }
            _ => return Err(CryptoFailure::Algorithm(alg)),
        };
        Ok(Self { state })
    }

    /// Feeds input bytes into the running MAC calculation.
    pub fn update(&mut self, data: &[u8]) {
        match &mut self.state {
            MacState::HmacSha256(m) => m.update(data),
            MacState::HmacSha384(m) => m.update(data),
            MacState::HmacSha512(m) => m.update(data),
            MacState::Blake3(b) => {
                let _ = b.update(data);
            }
        }
    }

    /// Finalizes the MAC and returns the tag bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        match self.state {
            MacState::HmacSha256(m) => m.finalize().into_bytes().to_vec(),
            MacState::HmacSha384(m) => m.finalize().into_bytes().to_vec(),
            MacState::HmacSha512(m) => m.finalize().into_bytes().to_vec(),
            MacState::Blake3(b) => b.finalize().as_bytes().to_vec(),
        }
    }

    /// Finalizes and verifies the tag in constant time.
    #[must_use]
    pub fn verify(self, tag: &[u8]) -> bool {
        let computed = self.finish();
        if computed.len() != tag.len() {
            return false;
        }
        computed.ct_eq(tag).into()
    }
}

/// Computes the one-shot MAC of `data` under `key` using `alg`.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if `alg` is invalid or unsupported,
/// or `CryptoFailure::Key` if `key` is invalid for the algorithm.
pub fn bn_mac(key: &[u8], data: &[u8], alg: i32) -> Result<Vec<u8>, CryptoFailure> {
    let mut stream = MacStream::new(key, alg)?;
    stream.update(data);
    Ok(stream.finish())
}

/// Verifies `tag` against `data` and `key` in constant time.
#[must_use]
pub fn bn_mac_verify(key: &[u8], data: &[u8], tag: &[u8], alg: i32) -> bool {
    let Ok(expected) = bn_mac(key, data, alg) else {
        return false;
    };
    if expected.len() != tag.len() {
        return false;
    }
    expected.ct_eq(tag).into()
}

static NEXT_STREAM_HANDLE: AtomicI64 = AtomicI64::new(1);
static STREAMS: Mutex<Option<HashMap<i64, MacStream>>> = Mutex::new(None);

fn with_streams<R>(f: impl FnOnce(&mut HashMap<i64, MacStream>) -> R) -> R {
    let mut guard = STREAMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

/// Inserts a `MacStream` into the global runtime table, returning a handle.
pub fn insert_mac_stream(stream: MacStream) -> i64 {
    let handle = NEXT_STREAM_HANDLE.fetch_add(1, Ordering::Relaxed);
    with_streams(|t| t.insert(handle, stream));
    handle
}

/// Updates an existing `MacStream` identified by `handle`.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn update_mac_stream(handle: i64, data: &[u8]) -> Result<(), CryptoFailure> {
    with_streams(|t| match t.get_mut(&handle) {
        Some(s) => {
            s.update(data);
            Ok(())
        }
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Finalizes and removes a `MacStream` identified by `handle`, returning its tag.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn finish_mac_stream(handle: i64) -> Result<Vec<u8>, CryptoFailure> {
    with_streams(|t| match t.remove(&handle) {
        Some(s) => Ok(s.finish()),
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Verifies a `MacStream` against `tag` in constant time, removing the stream.
pub fn verify_mac_stream(handle: i64, tag: &[u8]) -> bool {
    with_streams(|t| match t.remove(&handle) {
        Some(s) => s.verify(tag),
        None => false,
    })
}

/// Releases a `MacStream` handle without computing its tag.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle was not found.
pub fn release_mac_stream(handle: i64) -> Result<(), CryptoFailure> {
    with_streams(|t| match t.remove(&handle) {
        Some(_) => Ok(()),
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

// ---------------------------------------------------------------------------
// C ABI exports for MAC and streaming MacStream
// ---------------------------------------------------------------------------

use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed, store};

fn get_any_key(handle: u64) -> Option<zeroize::Zeroizing<Vec<u8>>> {
    if let Some(mat) = crate::crypto_secret_abi::with_secret_keys(|keys| {
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

/// C ABI: `Mac(key, data, alg)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac(
    key_handle: u64,
    data_ptr: *const u8,
    data_len: usize,
    alg: i32,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() || (data_ptr.is_null() && data_len > 0) {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(key_bytes) = get_any_key(key_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let data = if data_len == 0 {
        &[][..]
    } else {
        // SAFETY: checked non-null and valid for data_len bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, data_len) }
    };
    match bn_mac(&key_bytes, data, alg) {
        Ok(tag) => {
            let handle = store(tag);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Mac(key, data)` handle helper.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_handle(key_handle: u64, data_handle: u64) -> i64 {
    let Some(key_bytes) = get_any_key(key_handle) else {
        return 0;
    };
    let Some(data_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&data_handle).cloned())
    else {
        return 0;
    };
    match bn_mac(&key_bytes, &data_bytes, MAC_HMAC_SHA256) {
        Ok(tag) => {
            let handle = store(tag);
            i64::try_from(handle).unwrap_or(0)
        }
        Err(_) => 0,
    }
}

/// C ABI: `VerifyMac(key, data, tag, alg)`. Returns 1 for match, 0 for mismatch/error.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_verify(
    key_handle: u64,
    data_ptr: *const u8,
    data_len: usize,
    tag_handle: i64,
    alg: i32,
) -> i32 {
    let Ok(u_tag) = u64::try_from(tag_handle) else {
        return 0;
    };
    let Some(key_bytes) = get_any_key(key_handle) else {
        return 0;
    };
    let Some(tag_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&u_tag).cloned()) else {
        return 0;
    };
    let data = if data_len == 0 {
        &[][..]
    } else if data_ptr.is_null() {
        return 0;
    } else {
        // SAFETY: checked non-null and valid for data_len bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, data_len) }
    };
    i32::from(bn_mac_verify(&key_bytes, data, &tag_bytes, alg))
}

/// C ABI: `VerifyMac(key, data, tag)` handle helper.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_verify_handle(
    key_handle: u64,
    data_handle: u64,
    tag_handle: u64,
) -> i32 {
    let Some(key_bytes) = get_any_key(key_handle) else {
        return 0;
    };
    let Some(data_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&data_handle).cloned())
    else {
        return 0;
    };
    let Some(tag_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&tag_handle).cloned())
    else {
        return 0;
    };
    i32::from(bn_mac_verify(
        &key_bytes,
        &data_bytes,
        &tag_bytes,
        MAC_HMAC_SHA256,
    ))
}

const MAC_OPTS_ALG_OFFSET: usize = 16;

/// C ABI: `MacWith(key, data, opts)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_with(
    key_handle: u64,
    data_handle: u64,
    opts_ptr: *const u8,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(key_bytes) = get_any_key(key_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some(data_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&data_handle).cloned())
    else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let alg = if opts_ptr.is_null() {
        MAC_HMAC_SHA256
    } else {
        let mut alg_bytes = [0u8; 4];
        unsafe {
            std::ptr::copy_nonoverlapping(
                opts_ptr.add(MAC_OPTS_ALG_OFFSET),
                alg_bytes.as_mut_ptr(),
                4,
            );
        }
        i32::from_le_bytes(alg_bytes)
    };
    match bn_mac(&key_bytes, &data_bytes, alg) {
        Ok(tag) => {
            let handle = store(tag);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `MacStream.NewWith(key, opts)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_new_with(
    key_handle: u64,
    opts_ptr: *const u8,
    out_handle: *mut i64,
) -> i32 {
    let alg = if opts_ptr.is_null() {
        MAC_HMAC_SHA256
    } else {
        let mut alg_bytes = [0u8; 4];
        unsafe {
            std::ptr::copy_nonoverlapping(
                opts_ptr.add(MAC_OPTS_ALG_OFFSET),
                alg_bytes.as_mut_ptr(),
                4,
            );
        }
        i32::from_le_bytes(alg_bytes)
    };
    bn_rt_crypto_mac_stream_new(key_handle, alg, out_handle)
}

/// C ABI: `MacStream.New(key, alg)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_new(
    key_handle: u64,
    alg: i32,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(key_bytes) = get_any_key(key_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match MacStream::new(&key_bytes, alg) {
        Ok(stream) => {
            let handle = insert_mac_stream(stream);
            unsafe { *out_handle = handle };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `MacStream.Update(data_ptr, data_len)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_update(
    stream_handle: i64,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    if data_ptr.is_null() && data_len > 0 {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let data = if data_len == 0 {
        &[][..]
    } else {
        // SAFETY: pointer non-null and valid for data_len bytes.
        unsafe { std::slice::from_raw_parts(data_ptr, data_len) }
    };
    match update_mac_stream(stream_handle, data) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}

/// C ABI: `MacStream.Update(data_handle)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_update_handle(
    stream_handle: i64,
    data_handle: i64,
) -> i32 {
    let Ok(u_data) = u64::try_from(data_handle) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some(data_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&u_data).cloned())
    else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match update_mac_stream(stream_handle, &data_bytes) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}

/// C ABI: `MacStream.Finish()`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_finish(stream_handle: i64, out_handle: *mut i64) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    match finish_mac_stream(stream_handle) {
        Ok(tag) => {
            let handle = store(tag);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `MacStream.Verify(tag_handle)`. Returns 1 for match, 0 for mismatch/error.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_verify(stream_handle: i64, tag_handle: i64) -> i32 {
    let Ok(u_tag) = u64::try_from(tag_handle) else {
        return 0;
    };
    let Some(tag_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&u_tag).cloned()) else {
        return 0;
    };
    i32::from(verify_mac_stream(stream_handle, &tag_bytes))
}

/// C ABI: releases a `MacStream` handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_release(stream_handle: i64) -> i32 {
    match release_mac_stream(stream_handle) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}
