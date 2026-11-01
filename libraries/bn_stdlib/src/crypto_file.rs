// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Streaming authenticated file encryption and decryption using XChaCha20-Poly1305.
//!
//! Encrypts and authenticates files in 64 KiB segments with safe internal nonces
//! and versioned `BNC1` envelopes. Targets are created using owner-only atomic
//! creation (`FS.OpenNew`). Decryption verifies each chunk and deletes any
//! partial output file on authentication failure.

use std::ffi::c_char;
use std::io::{Read, Write};
use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use crate::c_str;
use crate::crypto_abi::failed;
use crate::crypto_envelope::{ENVELOPE_ALG_XCHACHA20_POLY1305, ENVELOPE_HEADER_LEN};
use crate::crypto_error::CryptoFailure;
use crate::crypto_secret_abi::with_secret_keys;

/// Segment plaintext size: 64 KiB.
pub const FILE_CHUNK_PLAIN_LEN: usize = 64 * 1024;
/// Authentication tag size: 16 bytes.
pub const FILE_CHUNK_TAG_LEN: usize = 16;
/// Segment ciphertext size for non-final chunks: 64 KiB + 16 bytes.
pub const FILE_CHUNK_CIPHER_MAX_LEN: usize = FILE_CHUNK_PLAIN_LEN + FILE_CHUNK_TAG_LEN;

fn fill_buffer(
    input: &mut impl Read,
    buf: &mut [u8],
    start: usize,
    max: usize,
) -> Result<usize, CryptoFailure> {
    let mut len = start;
    while len < max {
        match input.read(&mut buf[len..max]) {
            Ok(0) => break,
            Ok(n) => len += n,
            Err(_) => return Err(CryptoFailure::EnvelopeFormat),
        }
    }
    Ok(len)
}

fn derive_chunk_nonce(base_nonce: &[u8; 24], chunk_idx: u64, is_last: bool) -> XNonce {
    let mut nonce_bytes = [0u8; 24];
    nonce_bytes[0..15].copy_from_slice(&base_nonce[0..15]);
    nonce_bytes[15] = u8::from(is_last);
    let base_ctr = u64::from_be_bytes(
        base_nonce[16..24]
            .try_into()
            .expect("slice with correct length"),
    );
    nonce_bytes[16..24].copy_from_slice(&base_ctr.wrapping_add(chunk_idx).to_be_bytes());
    *XNonce::from_slice(&nonce_bytes)
}

fn encrypt_stream(
    cipher: &XChaCha20Poly1305,
    base_nonce: &[u8; 24],
    header: &[u8; ENVELOPE_HEADER_LEN],
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<(), CryptoFailure> {
    let mut chunk = vec![0u8; FILE_CHUNK_PLAIN_LEN];
    let mut chunk_len = fill_buffer(input, &mut chunk, 0, FILE_CHUNK_PLAIN_LEN)?;
    let mut chunk_idx = 0u64;
    let mut next_byte = [0u8; 1];

    loop {
        let n = input
            .read(&mut next_byte)
            .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;
        let is_last = n == 0;

        let nonce = derive_chunk_nonce(base_nonce, chunk_idx, is_last);
        let mut aad = Vec::with_capacity(ENVELOPE_HEADER_LEN + 8 + 1);
        aad.extend_from_slice(header);
        aad.extend_from_slice(&chunk_idx.to_be_bytes());
        aad.push(u8::from(is_last));

        let payload = Payload {
            msg: &chunk[..chunk_len],
            aad: &aad,
        };
        let ciphertext = cipher
            .encrypt(&nonce, payload)
            .map_err(|_| CryptoFailure::EnvelopeAuth)?;

        output
            .write_all(&ciphertext)
            .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

        chunk_idx += 1;
        if is_last {
            break;
        }

        chunk[0] = next_byte[0];
        chunk_len = fill_buffer(input, &mut chunk, 1, FILE_CHUNK_PLAIN_LEN)?;
    }
    Ok(())
}

fn decrypt_stream(
    cipher: &XChaCha20Poly1305,
    base_nonce: &[u8; 24],
    header: &[u8; ENVELOPE_HEADER_LEN],
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<(), CryptoFailure> {
    let mut chunk_buf = vec![0u8; FILE_CHUNK_CIPHER_MAX_LEN];
    let mut chunk_len = fill_buffer(input, &mut chunk_buf, 0, FILE_CHUNK_CIPHER_MAX_LEN)?;
    let mut chunk_idx = 0u64;
    let mut next_byte = [0u8; 1];

    loop {
        if chunk_len < FILE_CHUNK_TAG_LEN {
            return Err(CryptoFailure::EnvelopeFormat);
        }

        let Ok(n) = input.read(&mut next_byte) else {
            return Err(CryptoFailure::EnvelopeFormat);
        };
        let is_last = n == 0;

        if !is_last && chunk_len != FILE_CHUNK_CIPHER_MAX_LEN {
            return Err(CryptoFailure::EnvelopeFormat);
        }

        let nonce = derive_chunk_nonce(base_nonce, chunk_idx, is_last);
        let mut aad = Vec::with_capacity(ENVELOPE_HEADER_LEN + 8 + 1);
        aad.extend_from_slice(header);
        aad.extend_from_slice(&chunk_idx.to_be_bytes());
        aad.push(u8::from(is_last));

        let payload = Payload {
            msg: &chunk_buf[..chunk_len],
            aad: &aad,
        };

        let Ok(plaintext) = cipher.decrypt(&nonce, payload) else {
            return Err(CryptoFailure::EnvelopeAuth);
        };

        if output.write_all(&plaintext).is_err() {
            return Err(CryptoFailure::FileIo("write failed".into()));
        }

        chunk_idx += 1;
        if is_last {
            break;
        }

        chunk_buf[0] = next_byte[0];
        chunk_len = fill_buffer(input, &mut chunk_buf, 1, FILE_CHUNK_CIPHER_MAX_LEN)?;
    }
    Ok(())
}

/// Encrypts the file at `in_path` under `key` into `out_path` in streaming 64 KiB segments.
///
/// Output is created atomically with owner-only permissions (`FS.OpenNew` semantics).
/// If `out_path` already exists or cannot be created, fails without overwriting.
///
/// # Errors
///
/// Returns [`CryptoFailure::EnvelopeKeyLength`] if key is not 32 bytes,
/// [`CryptoFailure::Random`] if system entropy fails, or [`CryptoFailure::FileIo`]
/// on filesystem read/write errors.
pub fn bn_encrypt_file(key: &[u8], in_path: &Path, out_path: &Path) -> Result<(), CryptoFailure> {
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut base_nonce = [0u8; 24];
    getrandom::getrandom(&mut base_nonce)
        .map_err(|_| CryptoFailure::Random("BNCrypto.EncryptFile"))?;

    let mut header = [0u8; ENVELOPE_HEADER_LEN];
    header[0..4].copy_from_slice(b"BNC1");
    header[4] = 1; // version 1
    header[5] = ENVELOPE_ALG_XCHACHA20_POLY1305;
    header[6] = 1; // flags = STREAM
    header[7..31].copy_from_slice(&base_nonce);

    let mut input = crate::policy::open_path(in_path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let mut output = crate::policy::with_fs(|policy| policy.create_private_file(out_path))
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    output
        .write_all(&header)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;

    encrypt_stream(&cipher, &base_nonce, &header, &mut input, &mut output)?;

    output
        .sync_all()
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;
    Ok(())
}

/// Decrypts the streaming file at `in_path` under `key` into `out_path`.
///
/// Output is created atomically with owner-only permissions (`FS.OpenNew` semantics).
/// If authentication or decryption fails at any segment, the partial output file
/// is deleted immediately, failing closed.
///
/// # Errors
///
/// Returns [`CryptoFailure::EnvelopeKeyLength`] if key is not 32 bytes,
/// [`CryptoFailure::EnvelopeFormat`] on invalid header or truncated streams,
/// or [`CryptoFailure::EnvelopeAuth`] on authentication tag mismatch.
pub fn bn_decrypt_file(key: &[u8], in_path: &Path, out_path: &Path) -> Result<(), CryptoFailure> {
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut input = crate::policy::open_path(in_path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let mut header = [0u8; ENVELOPE_HEADER_LEN];
    if input.read_exact(&mut header).is_err() {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    if &header[0..4] != b"BNC1"
        || header[4] != 1
        || header[5] != ENVELOPE_ALG_XCHACHA20_POLY1305
        || header[6] != 1
    {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    let mut base_nonce = [0u8; 24];
    base_nonce.copy_from_slice(&header[7..31]);

    let mut output = crate::policy::with_fs(|policy| policy.create_private_file(out_path))
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let cleanup = || {
        let _ = crate::policy::with_fs(|policy| policy.remove_file(out_path));
    };

    let Ok(cipher) = XChaCha20Poly1305::new_from_slice(key) else {
        drop(output);
        cleanup();
        return Err(CryptoFailure::EnvelopeKeyLength);
    };

    if let Err(err) = decrypt_stream(&cipher, &base_nonce, &header, &mut input, &mut output) {
        drop(output);
        cleanup();
        return Err(err);
    }

    if output.sync_all().is_err() {
        drop(output);
        cleanup();
        return Err(CryptoFailure::FileIo("sync failed".into()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// C ABI exports for streaming file encryption
// ---------------------------------------------------------------------------

/// C ABI: `EncryptFile(key, inPath, outPath)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_encrypt_file(
    key_handle: u64,
    in_path: *const c_char,
    out_path: *const c_char,
) -> i32 {
    let Some(in_str) = c_str(in_path) else {
        return failed(&CryptoFailure::FileIo("invalid inPath".into()));
    };
    let Some(out_str) = c_str(out_path) else {
        return failed(&CryptoFailure::FileIo("invalid outPath".into()));
    };

    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key_bytes) = key_mat else {
        return failed(&CryptoFailure::Key("BNCrypto.EncryptFile"));
    };

    match bn_encrypt_file(&key_bytes, Path::new(in_str), Path::new(out_str)) {
        Ok(()) => 0,
        Err(err) => failed(&err),
    }
}

/// C ABI: `DecryptFile(key, inPath, outPath)`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_decrypt_file(
    key_handle: u64,
    in_path: *const c_char,
    out_path: *const c_char,
) -> i32 {
    let Some(in_str) = c_str(in_path) else {
        return failed(&CryptoFailure::FileIo("invalid inPath".into()));
    };
    let Some(out_str) = c_str(out_path) else {
        return failed(&CryptoFailure::FileIo("invalid outPath".into()));
    };

    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key_bytes) = key_mat else {
        return failed(&CryptoFailure::Key("BNCrypto.DecryptFile"));
    };

    match bn_decrypt_file(&key_bytes, Path::new(in_str), Path::new(out_str)) {
        Ok(()) => 0,
        Err(err) => failed(&err),
    }
}
