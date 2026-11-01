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

/// Encrypts `plaintext` under `key` into a versioned `BNC1` envelope with a safe random 24-byte nonce.
///
/// # Errors
///
/// Returns [`CryptoFailure::EnvelopeKeyLength`] if `key` is not 32 bytes,
/// [`CryptoFailure::Random`] if system entropy fails, or [`CryptoFailure::EnvelopeAuth`]
/// if encryption fails.
pub fn bn_encrypt_envelope(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoFailure> {
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|_| CryptoFailure::Random("BNCrypto.Encrypt"))?;

    let mut header = [0u8; ENVELOPE_HEADER_LEN];
    header[0..4].copy_from_slice(b"BNC1");
    header[4] = 1; // version 1
    header[5] = ENVELOPE_ALG_XCHACHA20_POLY1305;
    header[6] = 0; // flags
    header[7..31].copy_from_slice(&nonce);

    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;
    let payload = Payload {
        msg: plaintext,
        aad: &header,
    };
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), payload)
        .map_err(|_| CryptoFailure::EnvelopeAuth)?;

    let mut out = Vec::with_capacity(header.len() + ciphertext.len());
    out.extend_from_slice(&header);
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
    if key.len() != 32 {
        return Err(CryptoFailure::EnvelopeKeyLength);
    }
    if sealed.len() < ENVELOPE_MIN_LEN {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    if &sealed[0..4] != b"BNC1" {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    if sealed[4] != 1 {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    if sealed[5] != ENVELOPE_ALG_XCHACHA20_POLY1305 {
        return Err(CryptoFailure::EnvelopeFormat);
    }
    let header = &sealed[0..ENVELOPE_HEADER_LEN];
    let nonce = &header[7..31];
    let ciphertext_and_tag = &sealed[ENVELOPE_HEADER_LEN..];

    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoFailure::EnvelopeKeyLength)?;
    let payload = Payload {
        msg: ciphertext_and_tag,
        aad: header,
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
