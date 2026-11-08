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

use bn_rt::FsEntryPolicy as _;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use crate::c_str;
use crate::crypto_abi::failed;
use crate::crypto_envelope::{
    ENVELOPE_ALG_XCHACHA20_POLY1305, ENVELOPE_FLAG_PASSWORD, ENVELOPE_FLAG_STREAM,
};
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

/// Bytes of the PASSWORD section: the Argon2id `m`, `t`, `p` as little-endian
/// `u32`, then the salt.
const KDF_COST_LEN: usize = 12;
const KDF_SALT_LEN: usize = crate::crypto_password::DERIVE_KEY_MIN_SALT;
const KDF_SECTION_LEN: usize = KDF_COST_LEN + KDF_SALT_LEN;
/// Bytes of the fixed header before any section: magic, version, algorithm, flags.
const STREAM_FIXED_LEN: usize = 7;
const STREAM_NONCE_LEN: usize = 24;
const STREAM_MAGIC: &[u8; 4] = b"BNC1";
const STREAM_VERSION: u8 = 1;

fn associated_data(header: &[u8], chunk_idx: u64, is_last: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(header.len() + 8 + 1);
    aad.extend_from_slice(header);
    aad.extend_from_slice(&chunk_idx.to_be_bytes());
    aad.push(u8::from(is_last));
    aad
}

fn encrypt_stream(
    cipher: &XChaCha20Poly1305,
    base_nonce: &[u8; 24],
    header: &[u8],
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
        let aad = associated_data(header, chunk_idx, is_last);
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

/// Opens every segment. A first segment that does not authenticate is
/// `first_failure` (a wrong password names itself); a later one is `EnvelopeAuth`.
fn decrypt_stream(
    cipher: &XChaCha20Poly1305,
    base_nonce: &[u8; 24],
    header: &[u8],
    input: &mut impl Read,
    output: &mut impl Write,
    first_failure: &CryptoFailure,
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
        let aad = associated_data(header, chunk_idx, is_last);
        let payload = Payload {
            msg: &chunk_buf[..chunk_len],
            aad: &aad,
        };

        let Ok(plaintext) = cipher.decrypt(&nonce, payload) else {
            return Err(if chunk_idx == 0 {
                first_failure.clone()
            } else {
                CryptoFailure::EnvelopeAuth
            });
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

/// The fixed header of a stream with `flags`: magic, version, algorithm, flags.
fn stream_prefix(flags: u8) -> Vec<u8> {
    let mut prefix = Vec::with_capacity(STREAM_FIXED_LEN + KDF_SECTION_LEN + STREAM_NONCE_LEN);
    prefix.extend_from_slice(STREAM_MAGIC);
    prefix.push(STREAM_VERSION);
    prefix.push(ENVELOPE_ALG_XCHACHA20_POLY1305);
    prefix.push(flags);
    prefix
}

/// Seals `in_path` into `out_path`: `prefix` (fixed header and sections), a
/// random base nonce, then the segments.
fn seal_file(
    fs: &crate::FsPolicy,
    key: &[u8],
    mut header: Vec<u8>,
    in_path: &Path,
    out_path: &Path,
    operation: &'static str,
) -> Result<(), CryptoFailure> {
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut base_nonce = [0u8; STREAM_NONCE_LEN];
    getrandom::getrandom(&mut base_nonce).map_err(|_| CryptoFailure::Random(operation))?;
    header.extend_from_slice(&base_nonce);

    let mut input = fs
        .open(in_path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let mut output = fs
        .create_private_file(out_path)
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

/// Reads the fixed header of a stream; its flags must be exactly `flags`.
fn read_prefix(input: &mut impl Read, flags: u8) -> Result<Vec<u8>, CryptoFailure> {
    let mut prefix = vec![0u8; STREAM_FIXED_LEN];
    input
        .read_exact(&mut prefix)
        .map_err(|_| CryptoFailure::EnvelopeFormat)?;
    if prefix[..STREAM_MAGIC.len()] != STREAM_MAGIC[..]
        || prefix[STREAM_MAGIC.len()] != STREAM_VERSION
        || prefix[STREAM_MAGIC.len() + 1] != ENVELOPE_ALG_XCHACHA20_POLY1305
        || prefix[STREAM_FIXED_LEN - 1] != flags
    {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    Ok(prefix)
}

/// Opens the segments after `header` (read up to the nonce) into `out_path`;
/// any failure removes the partial output.
fn open_file(
    fs: &crate::FsPolicy,
    key: &[u8],
    mut header: Vec<u8>,
    input: &mut impl Read,
    out_path: &Path,
    first_failure: &CryptoFailure,
) -> Result<(), CryptoFailure> {
    let mut base_nonce = [0u8; STREAM_NONCE_LEN];
    input
        .read_exact(&mut base_nonce)
        .map_err(|_| CryptoFailure::EnvelopeFormat)?;
    header.extend_from_slice(&base_nonce);

    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;
    let mut output = fs
        .create_private_file(out_path)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;

    let result = decrypt_stream(
        &cipher,
        &base_nonce,
        &header,
        input,
        &mut output,
        first_failure,
    )
    .and_then(|()| {
        output
            .sync_all()
            .map_err(|_| CryptoFailure::FileIo("sync failed".into()))
    });
    if result.is_err() {
        drop(output);
        let _ = fs.remove_file(out_path);
    }
    result
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
pub fn bn_encrypt_file(
    fs: &crate::FsPolicy,
    key: &[u8],
    in_path: &Path,
    out_path: &Path,
) -> Result<(), CryptoFailure> {
    seal_file(
        fs,
        key,
        stream_prefix(ENVELOPE_FLAG_STREAM),
        in_path,
        out_path,
        bn_catalog::text("crypto", "op.encrypt_file"),
    )
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
/// [`CryptoFailure::EnvelopeFormat`] on invalid header (a password file included)
/// or truncated streams, or [`CryptoFailure::EnvelopeAuth`] on authentication tag mismatch.
pub fn bn_decrypt_file(
    fs: &crate::FsPolicy,
    key: &[u8],
    in_path: &Path,
    out_path: &Path,
) -> Result<(), CryptoFailure> {
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut input = fs
        .open(in_path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;
    let header = read_prefix(&mut input, ENVELOPE_FLAG_STREAM)?;
    open_file(
        fs,
        key,
        header,
        &mut input,
        out_path,
        &CryptoFailure::EnvelopeAuth,
    )
}

/// `EncryptFileWithPassword`: a 16-byte random salt, the key `DeriveKey`
/// derives from `password` and that salt, and a stream whose PASSWORD section
/// records the floor cost and the salt.
///
/// # Errors
///
/// As [`bn_encrypt_file`].
pub fn bn_encrypt_file_with_password(
    fs: &crate::FsPolicy,
    password: &str,
    in_path: &Path,
    out_path: &Path,
) -> Result<(), CryptoFailure> {
    let operation = bn_catalog::text("crypto", "op.encrypt_file_with_password");
    let mut salt = [0u8; KDF_SALT_LEN];
    getrandom::getrandom(&mut salt).map_err(|_| CryptoFailure::Random(operation))?;
    let key = crate::crypto_password::bn_derive_key(password, &salt)?;
    let floor = crate::crypto_password::PasswordCost::FLOOR;
    let mut header = stream_prefix(ENVELOPE_FLAG_STREAM | ENVELOPE_FLAG_PASSWORD);
    for field in [floor.memory_kib, floor.iterations, floor.parallelism] {
        header.extend_from_slice(&field.to_le_bytes());
    }
    header.extend_from_slice(&salt);
    seal_file(fs, &key, header, in_path, out_path, operation)
}

/// `DecryptFileWithPassword`: derives the key from the recorded cost and salt.
/// Only the floor cost is accepted, so a hostile header cannot demand memory.
///
/// # Errors
///
/// `WrongPassword` when the first segment does not authenticate,
/// `PasswordCost` for a recorded cost other than the floor, `EnvelopeFormat`
/// for a stream without the PASSWORD section, and the failures of
/// [`bn_decrypt_file`].
pub fn bn_decrypt_file_with_password(
    fs: &crate::FsPolicy,
    password: &str,
    in_path: &Path,
    out_path: &Path,
) -> Result<(), CryptoFailure> {
    let operation = bn_catalog::text("crypto", "op.decrypt_file_with_password");
    let mut input = fs
        .open(in_path, crate::secure_fs::OpenMode::Read)
        .map_err(|e| CryptoFailure::FileIo(e.to_string()))?;
    let mut header = read_prefix(&mut input, ENVELOPE_FLAG_STREAM | ENVELOPE_FLAG_PASSWORD)?;
    let mut section = [0u8; KDF_SECTION_LEN];
    input
        .read_exact(&mut section)
        .map_err(|_| CryptoFailure::EnvelopeFormat)?;
    let field = |at: usize| {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&section[at..at + 4]);
        u32::from_le_bytes(bytes)
    };
    let recorded = crate::crypto_password::PasswordCost {
        memory_kib: field(0),
        iterations: field(4),
        parallelism: field(8),
    };
    if recorded != crate::crypto_password::PasswordCost::FLOOR {
        return Err(CryptoFailure::PasswordCost { operation });
    }
    let key = crate::crypto_password::bn_derive_key(password, &section[KDF_COST_LEN..])?;
    header.extend_from_slice(&section);
    open_file(
        fs,
        &key,
        header,
        &mut input,
        out_path,
        &CryptoFailure::WrongPassword { operation },
    )
}

// ---------------------------------------------------------------------------
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

    match crate::policy::with_fs(|fs| {
        bn_encrypt_file(fs, &key_bytes, Path::new(in_str), Path::new(out_str))
    }) {
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

    match crate::policy::with_fs(|fs| {
        bn_decrypt_file(fs, &key_bytes, Path::new(in_str), Path::new(out_str))
    }) {
        Ok(()) => 0,
        Err(err) => failed(&err),
    }
}

/// The STRING arguments of a password-file call: input path, output path,
/// password. A pointer that is not a STRING is the call's I/O failure.
fn password_file_arguments<'a>(
    in_path: *const c_char,
    out_path: *const c_char,
    password: *const c_char,
) -> Result<(&'a str, &'a str, &'a str), CryptoFailure> {
    let text = |pointer: *const c_char| {
        c_str(pointer).ok_or_else(|| {
            CryptoFailure::FileIo(bn_catalog::text("crypto", "file.argument.not_string").into())
        })
    };
    Ok((text(in_path)?, text(out_path)?, text(password)?))
}

/// C ABI: `EncryptFileWithPassword(inPath, outPath, password) AS VOID OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_encrypt_file_with_password(
    in_path: *const c_char,
    out_path: *const c_char,
    password: *const c_char,
) -> i32 {
    let result = password_file_arguments(in_path, out_path, password).and_then(
        |(input, output, password)| {
            crate::policy::with_fs(|fs| {
                bn_encrypt_file_with_password(fs, password, Path::new(input), Path::new(output))
            })
        },
    );
    match result {
        Ok(()) => 0,
        Err(err) => failed(&err),
    }
}

/// C ABI: `DecryptFileWithPassword(inPath, outPath, password) AS VOID OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_decrypt_file_with_password(
    in_path: *const c_char,
    out_path: *const c_char,
    password: *const c_char,
) -> i32 {
    let result = password_file_arguments(in_path, out_path, password).and_then(
        |(input, output, password)| {
            crate::policy::with_fs(|fs| {
                bn_decrypt_file_with_password(fs, password, Path::new(input), Path::new(output))
            })
        },
    );
    match result {
        Ok(()) => 0,
        Err(err) => failed(&err),
    }
}
