// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Versioned authenticated encryption envelopes (`BNC1`) using XChaCha20-Poly1305.

use std::ffi::c_char;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use crate::crypto_abi::{failed, store, with_buffers};
use crate::crypto_csprng::{decode_base64url, encode_base64url_unpadded};
use crate::crypto_error::CryptoFailure;
use crate::crypto_secret_abi::with_secret_keys;
use crate::{c_str, c_string};

/// Envelope algorithm constant for XChaCha20-Poly1305.
pub const ENVELOPE_ALG_XCHACHA20_POLY1305: u8 = 1;

/// Length of the BNC1 envelope header in bytes:
/// 4 bytes magic (`"BNC1"`) + 1 byte version (`0x01`) + 1 byte alg ID (`0x01`) + 1 byte flags (`0x00`) + 24 bytes nonce.
pub const ENVELOPE_HEADER_LEN: usize = 31;
pub const ENVELOPE_TAG_LEN: usize = 16;
pub const ENVELOPE_MIN_LEN: usize = ENVELOPE_HEADER_LEN + ENVELOPE_TAG_LEN;

/// Flags bit 0: the body is a file stream of segments (`EncryptFile`).
pub const ENVELOPE_FLAG_STREAM: u8 = 0b001;
/// Flags bit 1: the 32-byte KeyId of the key follows the flags.
pub const ENVELOPE_FLAG_KEYID: u8 = 0b010;
/// Flags bit 2: the Argon2id cost and salt follow (`EncryptFileWithPassword`).
pub const ENVELOPE_FLAG_PASSWORD: u8 = 0b100;

const ENVELOPE_MAGIC: &[u8; 4] = b"BNC1";
const ENVELOPE_VERSION: u8 = 1;
const ENVELOPE_VERSION_AT: usize = 4;
const ENVELOPE_ALG_AT: usize = 5;
const ENVELOPE_FLAGS_AT: usize = 6;
/// Bytes before the optional sections: magic, version, algorithm, flags.
const ENVELOPE_FIXED_LEN: usize = ENVELOPE_FLAGS_AT + 1;
/// Bytes of the KEYID section.
pub const ENVELOPE_KEY_ID_LEN: usize = 32;
/// Bytes of the XChaCha20-Poly1305 nonce.
pub const ENVELOPE_NONCE_LEN: usize = 24;
/// Bytes of a 256-bit key.
const ENVELOPE_KEY_LEN: usize = 32;

/// What `EncryptWith` takes from `AeadOptions` once the algorithm is accepted.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvelopeOptions<'a> {
    /// Caller data authenticated after the header; it is not stored.
    pub aad: &'a [u8],
    /// Whether the KEYID section is written.
    pub include_key_id: bool,
}

/// The authenticated data of an envelope: every header byte, then the caller's.
fn associated_data(header: &[u8], aad: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(header.len() + aad.len());
    data.extend_from_slice(header);
    data.extend_from_slice(aad);
    data
}

/// Encrypts `plaintext` under `key` into a versioned `BNC1` envelope with a safe random 24-byte nonce.
///
/// # Errors
///
/// Returns [`CryptoFailure::EnvelopeKeyLength`] if `key` is not 32 bytes,
/// [`CryptoFailure::Random`] if system entropy fails, or [`CryptoFailure::EnvelopeAuth`]
/// if encryption fails.
pub fn bn_encrypt_envelope(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    bn_encrypt_envelope_with(key, plaintext, &EnvelopeOptions::default())
}

/// `EncryptWith`: a `BNC1` envelope whose flags record the KEYID section when
/// `options` asks for it, and whose AAD is the header followed by `options.aad`.
///
/// # Errors
///
/// As [`bn_encrypt_envelope`].
pub fn bn_encrypt_envelope_with(
    key: &[u8],
    plaintext: &[u8],
    options: &EnvelopeOptions<'_>,
) -> Result<Vec<u8>, CryptoFailure> {
    if key.len() != ENVELOPE_KEY_LEN {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut nonce = [0u8; ENVELOPE_NONCE_LEN];
    getrandom::getrandom(&mut nonce)
        .map_err(|_| CryptoFailure::Random(bn_catalog::text("crypto", "op.encrypt")))?;
    let mut header = Vec::with_capacity(ENVELOPE_HEADER_LEN + ENVELOPE_KEY_ID_LEN);
    header.extend_from_slice(ENVELOPE_MAGIC);
    header.push(ENVELOPE_VERSION);
    header.push(ENVELOPE_ALG_XCHACHA20_POLY1305);
    if options.include_key_id {
        header.push(ENVELOPE_FLAG_KEYID);
        header.extend_from_slice(&crate::crypto_secret_abi::secret_key_id_bytes(key));
    } else {
        header.push(0);
    }
    header.extend_from_slice(&nonce);

    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;
    let aad = associated_data(&header, options.aad);
    let payload = Payload {
        msg: plaintext,
        aad: &aad,
    };
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), payload)
        .map_err(|_| CryptoFailure::EnvelopeAuth)?;

    let mut out = header;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Verifies and decrypts a `BNC1` envelope under `key`.
///
/// # Errors
///
/// Returns [`CryptoFailure::EnvelopeKeyLength`] if `key` is not 32 bytes,
/// [`CryptoFailure::EnvelopeFormat`] if envelope is malformed,
/// or [`CryptoFailure::EnvelopeAuth`] if authentication fails.
pub fn bn_decrypt_envelope(key: &[u8], sealed: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    bn_decrypt_envelope_with(key, sealed, &[])
}

/// `DecryptWith`: opens a single `BNC1` envelope sealed with the caller data
/// `aad`. A KEYID that is not the key's fails as a wrong key does.
///
/// # Errors
///
/// As [`bn_decrypt_envelope`]; a STREAM or PASSWORD flag, an unknown flag bit,
/// or an input too short for its sections is [`CryptoFailure::EnvelopeFormat`].
pub fn bn_decrypt_envelope_with(
    key: &[u8],
    sealed: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoFailure> {
    if key.len() != ENVELOPE_KEY_LEN {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    if sealed.len() < ENVELOPE_FIXED_LEN
        || &sealed[..ENVELOPE_MAGIC.len()] != ENVELOPE_MAGIC
        || sealed[ENVELOPE_VERSION_AT] != ENVELOPE_VERSION
        || sealed[ENVELOPE_ALG_AT] != ENVELOPE_ALG_XCHACHA20_POLY1305
    {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    let flags = sealed[ENVELOPE_FLAGS_AT];
    // A single envelope carries only the KEYID section: a STREAM or PASSWORD
    // flag, or a bit outside the three, is not one.
    if flags & !ENVELOPE_FLAG_KEYID != 0 {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    let key_id_len = if flags & ENVELOPE_FLAG_KEYID == 0 {
        0
    } else {
        ENVELOPE_KEY_ID_LEN
    };
    let nonce_at = ENVELOPE_FIXED_LEN + key_id_len;
    let header_len = nonce_at + ENVELOPE_NONCE_LEN;
    if sealed.len() < header_len + ENVELOPE_TAG_LEN {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    let header = &sealed[..header_len];
    if key_id_len != 0
        && header[ENVELOPE_FIXED_LEN..nonce_at]
            != crate::crypto_secret_abi::secret_key_id_bytes(key)[..]
    {
        return Err(CryptoFailure::EnvelopeAuth);
    }
    let nonce = &header[nonce_at..];
    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;
    let associated = associated_data(header, aad);
    let payload = Payload {
        msg: &sealed[header_len..],
        aad: &associated,
    };
    cipher
        .decrypt(XNonce::from_slice(nonce), payload)
        .map_err(|_| CryptoFailure::EnvelopeAuth)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_encrypt(key_handle: u64, plaintext_handle: u64) -> u64 {
    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key) = key_mat else {
        return 0;
    };
    let pt = with_buffers(|b| b.get(&plaintext_handle).cloned());
    let Some(plaintext) = pt else {
        return 0;
    };
    match bn_encrypt_envelope(&key, &plaintext) {
        Ok(sealed) => store(sealed),
        Err(err) => {
            failed(&err);
            0
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_decrypt(key_handle: u64, sealed_handle: u64) -> u64 {
    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key) = key_mat else {
        return 0;
    };
    let ct = with_buffers(|b| b.get(&sealed_handle).cloned());
    let Some(sealed) = ct else {
        return 0;
    };
    match bn_decrypt_envelope(&key, &sealed) {
        Ok(plain) => store(plain),
        Err(err) => {
            failed(&err);
            0
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_encrypt_text(key_handle: u64, text: *const c_char) -> *mut c_char {
    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key) = key_mat else {
        return std::ptr::null_mut();
    };
    let Some(input_str) = c_str(text) else {
        return std::ptr::null_mut();
    };
    match bn_encrypt_envelope(&key, input_str.as_bytes()) {
        Ok(sealed) => {
            let token = encode_base64url_unpadded(&sealed);
            c_string(&token)
        }
        Err(err) => {
            failed(&err);
            std::ptr::null_mut()
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_decrypt_text(
    key_handle: u64,
    sealed_text: *const c_char,
) -> *mut c_char {
    let key_mat = with_secret_keys(|keys| {
        let entry = keys.get(&key_handle)?;
        if entry.zeroized {
            return None;
        }
        Some(entry.material.clone())
    });
    let Some(key) = key_mat else {
        return std::ptr::null_mut();
    };
    let Some(token_str) = c_str(sealed_text) else {
        return std::ptr::null_mut();
    };
    let Some(sealed_bytes) = decode_base64url(token_str) else {
        return std::ptr::null_mut();
    };
    match bn_decrypt_envelope(&key, &sealed_bytes) {
        Ok(plain) => match String::from_utf8(plain) {
            Ok(utf8_str) => c_string(&utf8_str),
            Err(_) => std::ptr::null_mut(),
        },
        Err(err) => {
            failed(&err);
            std::ptr::null_mut()
        }
    }
}
