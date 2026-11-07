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

struct HeldStream {
    stream: MacStream,
    holders: u32,
}

static NEXT_STREAM_HANDLE: AtomicI64 = AtomicI64::new(1);
static STREAMS: Mutex<Option<HashMap<i64, HeldStream>>> = Mutex::new(None);

fn with_streams<R>(f: impl FnOnce(&mut HashMap<i64, HeldStream>) -> R) -> R {
    let mut guard = STREAMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

/// Inserts a `MacStream` into the global runtime table, returning a handle.
/// The new handle has one holder.
pub fn insert_mac_stream(stream: MacStream) -> i64 {
    let handle = NEXT_STREAM_HANDLE.fetch_add(1, Ordering::Relaxed);
    with_streams(|t| {
        t.insert(handle, HeldStream { stream, holders: 1 });
    });
    handle
}

/// One more binding holds `handle`. Handle 0 holds nothing.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn retain_mac_stream(handle: i64) -> Result<(), CryptoFailure> {
    if handle == 0 {
        return Ok(());
    }
    with_streams(|t| match t.get_mut(&handle) {
        Some(held) => {
            held.holders = held.holders.saturating_add(1);
            Ok(())
        }
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Updates an existing `MacStream` identified by `handle`.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn update_mac_stream(handle: i64, data: &[u8]) -> Result<(), CryptoFailure> {
    with_streams(|t| match t.get_mut(&handle) {
        Some(held) => {
            held.stream.update(data);
            Ok(())
        }
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Finalizes and removes a `MacStream` identified by `handle`, returning its tag.
/// Finish consumes the stream for every holder.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle is invalid or was released.
pub fn finish_mac_stream(handle: i64) -> Result<Vec<u8>, CryptoFailure> {
    with_streams(|t| match t.remove(&handle) {
        Some(held) => Ok(held.stream.finish()),
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

/// Verifies a `MacStream` against `tag` in constant time, removing the stream.
/// Verify consumes the stream for every holder.
pub fn verify_mac_stream(handle: i64, tag: &[u8]) -> bool {
    with_streams(|t| match t.remove(&handle) {
        Some(held) => held.stream.verify(tag),
        None => false,
    })
}

/// Drops one holder of `handle`. The stream is removed at zero holders.
/// Returns whether the entry is gone. Handle 0 holds nothing.
///
/// # Errors
/// Returns `CryptoFailure::Algorithm` if the handle was not found.
pub fn release_mac_stream(handle: i64) -> Result<bool, CryptoFailure> {
    if handle == 0 {
        return Ok(false);
    }
    with_streams(|t| match t.get_mut(&handle) {
        Some(held) => {
            held.holders = held.holders.saturating_sub(1);
            if held.holders == 0 {
                t.remove(&handle);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        None => Err(CryptoFailure::Algorithm(0)),
    })
}

// ---------------------------------------------------------------------------
// C ABI exports for MAC and streaming MacStream
// ---------------------------------------------------------------------------

use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed, store};

/// C ABI: `VerifyMac(key, data, tag)` handle helper.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_verify_handle(
    key_handle: u64,
    data_handle: u64,
    tag_handle: u64,
) -> i32 {
    let Ok(key_bytes) = crate::crypto_abi::any_key_material(
        key_handle,
        bn_catalog::text("crypto", "op.verify_mac"),
    ) else {
        // A zeroized or unknown key authenticates nothing (fail-closed).
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
    let operation = bn_catalog::text("crypto", "op.mac");
    let key_bytes = match crate::crypto_abi::any_key_material(key_handle, operation) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    let Some(data_bytes) = crate::crypto_abi::with_buffers(|bufs| bufs.get(&data_handle).cloned())
    else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let alg = if opts_ptr.is_null() {
        MAC_HMAC_SHA256
    } else {
        // SAFETY: `opts_ptr` is the non-null `MacOptions` record the backend laid
        // out; `alg` is its `INT32` at `MAC_OPTS_ALG_OFFSET`.
        unsafe { crate::crypto_policy::read_option_i32(opts_ptr, MAC_OPTS_ALG_OFFSET) }
    };
    // SAFETY: `opts_ptr` is null or the `MacOptions` record the backend laid out.
    let policy = unsafe {
        crate::crypto_policy::read_policy(opts_ptr, crate::crypto_policy::MAC_OPTS_POLICY_OFFSET)
    };
    if let Err(failure) = policy.admits_mac(alg) {
        return failed(&failure);
    }
    match bn_mac(&key_bytes, &data_bytes, alg) {
        Ok(tag) => {
            let handle = store(tag);
            let handle_i64 = i64::try_from(handle).unwrap_or(i64::MAX);
            // SAFETY: `out_handle` was checked non-null at entry; it is the caller's slot.
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
        // SAFETY: `opts_ptr` is the non-null `MacOptions` record the backend laid
        // out; `alg` is its `INT32` at `MAC_OPTS_ALG_OFFSET`.
        unsafe { crate::crypto_policy::read_option_i32(opts_ptr, MAC_OPTS_ALG_OFFSET) }
    };
    // SAFETY: `opts_ptr` is null or the `MacOptions` record the backend laid out.
    let policy = unsafe {
        crate::crypto_policy::read_policy(opts_ptr, crate::crypto_policy::MAC_OPTS_POLICY_OFFSET)
    };
    if let Err(failure) = policy.admits_mac(alg) {
        return failed(&failure);
    }
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
    let key_bytes = match crate::crypto_abi::any_key_material(
        key_handle,
        bn_catalog::text("crypto", "op.mac_stream_new"),
    ) {
        Ok(material) => material,
        Err(failure) => return failed(&failure),
    };
    match MacStream::new(&key_bytes, alg) {
        Ok(stream) => {
            let handle = insert_mac_stream(stream);
            // SAFETY: `out_handle` was checked non-null at entry; it is the caller's slot.
            unsafe { *out_handle = handle };
            BN_CRYPTO_OK
        }
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
            // SAFETY: `out_handle` was checked non-null at entry; it is the caller's slot.
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

/// C ABI: one more binding holds the stream. Handle 0 holds nothing.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_retain(stream_handle: i64) -> i32 {
    match retain_mac_stream(stream_handle) {
        Ok(()) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}

/// C ABI: releases one holder of a `MacStream`. Handle 0 holds nothing.
/// A missing handle was finished or verified.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_mac_stream_release(stream_handle: i64) -> i32 {
    match release_mac_stream(stream_handle) {
        Ok(_) => BN_CRYPTO_OK,
        Err(err) => failed(&err),
    }
}
