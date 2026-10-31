// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! OS-backed CSPRNG utilities for Basic Next cryptography:
//! unbiased random integers via rejection sampling, URL-safe base64 tokens,
//! RFC 4122 `UUIDv4`, RFC 9562 `UUIDv7`, and structured password generation.

use crate::crypto_error::CryptoFailure;

/// The most bytes one `RandomBytes` call returns (`bncrypto.md`): keys,
/// seeds and nonces need tens of bytes, and the bound keeps a mistaken
/// length from exhausting memory.
pub const MAX_RANDOM_BYTES: usize = 1024 * 1024;

/// Generates `length` random bytes using the operating system CSPRNG (`getrandom`).
///
/// # Errors
///
/// `RandomLength` for a length outside 0 through [`MAX_RANDOM_BYTES`];
/// `Random` when the system entropy source fails.
pub fn random_bytes(length: i128) -> Result<Vec<u8>, CryptoFailure> {
    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length <= MAX_RANDOM_BYTES)
        .ok_or(CryptoFailure::RandomLength(length))?;
    let mut buf = vec![0u8; length];
    getrandom::getrandom(&mut buf).map_err(|_| CryptoFailure::Random("BNCrypto.RandomBytes"))?;
    Ok(buf)
}

/// Generates an unbiased random integer in `[min, max]` via rejection sampling.
///
/// # Errors
///
/// `RandomIntRange` when `min > max`; `Random` when system entropy fails.
pub fn random_int(min: i64, max: i64) -> Result<i64, CryptoFailure> {
    if min > max {
        return Err(CryptoFailure::RandomIntRange { min, max });
    }
    if min == max {
        return Ok(min);
    }
    let diff_i128 = i128::from(max) - i128::from(min);
    let diff = u64::try_from(diff_i128).unwrap_or(u64::MAX);
    if diff == u64::MAX {
        let mut buf = [0u8; 8];
        getrandom::getrandom(&mut buf).map_err(|_| CryptoFailure::Random("BNCrypto.RandomInt"))?;
        return Ok(i64::from_le_bytes(buf));
    }
    let n = diff + 1;
    let zone = u64::MAX - (u64::MAX % n);
    let mut buf = [0u8; 8];
    loop {
        getrandom::getrandom(&mut buf).map_err(|_| CryptoFailure::Random("BNCrypto.RandomInt"))?;
        let x = u64::from_le_bytes(buf);
        if x < zone {
            return Ok(min.wrapping_add((x % n).cast_signed()));
        }
    }
}

const BASE64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodes `bytes` as an unpadded URL-safe base64 string (RFC 4648 §5).
#[must_use]
pub fn encode_base64url_unpadded(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let (chunks, rem) = bytes.as_chunks::<3>();
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        out.push(BASE64URL_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64URL_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(BASE64URL_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(BASE64URL_CHARS[(b2 & 0x3f) as usize] as char);
    }
    match rem.len() {
        1 => {
            let b0 = rem[0];
            out.push(BASE64URL_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64URL_CHARS[((b0 & 0x03) << 4) as usize] as char);
        }
        2 => {
            let b0 = rem[0];
            let b1 = rem[1];
            out.push(BASE64URL_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64URL_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(BASE64URL_CHARS[((b1 & 0x0f) << 2) as usize] as char);
        }
        _ => {}
    }
    out
}

/// Generates a URL-safe base64 string from `n_bytes` random bytes without padding.
///
/// # Errors
///
/// `RandomTokenLength` when `n_bytes <= 0` or exceeds [`MAX_RANDOM_BYTES`].
pub fn random_token(n_bytes: i64) -> Result<String, CryptoFailure> {
    let length = usize::try_from(n_bytes)
        .ok()
        .filter(|&len| len > 0 && len <= MAX_RANDOM_BYTES)
        .ok_or(CryptoFailure::RandomTokenLength(n_bytes))?;
    let mut buf = vec![0u8; length];
    getrandom::getrandom(&mut buf).map_err(|_| CryptoFailure::Random("BNCrypto.RandomToken"))?;
    Ok(encode_base64url_unpadded(&buf))
}

fn format_uuid(b: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    )
}

/// Generates a canonical RFC 4122 UUID version 4 string using CSPRNG entropy.
pub fn random_uuid_v4() -> Result<String, CryptoFailure> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| CryptoFailure::Random("BNCrypto.RandomUuidV4"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format_uuid(&bytes))
}

static UUID_V7_STATE: std::sync::Mutex<(u64, u16)> = std::sync::Mutex::new((0, 0));

/// Generates a time-ordered RFC 9562 UUID version 7 string.
///
/// Guaranteed to be monotonically increasing even when generated within the
/// same millisecond, using an RFC 9562 monotonic counter.
pub fn uuid_v7() -> Result<String, CryptoFailure> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| CryptoFailure::Random("BNCrypto.UuidV7"))?;
    let millis = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CryptoFailure::Random("BNCrypto.UuidV7"))?
            .as_millis(),
    )
    .unwrap_or(0);

    let (ms, counter) = {
        let mut state = UUID_V7_STATE
            .lock()
            .map_err(|_| CryptoFailure::Random("BNCrypto.UuidV7"))?;
        if millis > state.0 {
            let initial_counter = u16::from_be_bytes([bytes[6] & 0x0f, bytes[7]]) & 0x07ff;
            *state = (millis, initial_counter);
        } else {
            // Same millisecond or backward clock step: increment counter monotonically
            state.1 = (state.1 + 1) & 0x0fff;
        }
        *state
    };

    let ms_bytes = ms.to_be_bytes();
    bytes[0..6].copy_from_slice(&ms_bytes[2..8]);
    let counter_bytes = counter.to_be_bytes();
    // Version 7 (0111) in high nibble + 4 counter bits in low nibble
    bytes[6] = 0x70 | (counter_bytes[0] & 0x0f);
    bytes[7] = counter_bytes[1];
    // Variant RFC 9562 (10xx)
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format_uuid(&bytes))
}

/// Options for password generation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordGenOptions {
    pub length: usize,
    pub include_uppercase: bool,
    pub include_lowercase: bool,
    pub include_digits: bool,
    pub include_symbols: bool,
}

impl Default for PasswordGenOptions {
    fn default() -> Self {
        Self {
            length: 20,
            include_uppercase: true,
            include_lowercase: true,
            include_digits: true,
            include_symbols: true,
        }
    }
}

const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{}|;:,.<>?~";

/// Generates a secure random password with requested length and character classes.
///
/// # Errors
///
/// `PasswordLength` if `length == 0`; `PasswordNoClasses` if all character classes are disabled.
pub fn random_password(opts: PasswordGenOptions) -> Result<String, CryptoFailure> {
    if opts.length == 0 {
        return Err(CryptoFailure::PasswordLength(0));
    }
    if !opts.include_uppercase
        && !opts.include_lowercase
        && !opts.include_digits
        && !opts.include_symbols
    {
        return Err(CryptoFailure::PasswordNoClasses);
    }
    let mut pool: Vec<u8> = Vec::with_capacity(92);
    let mut required: Vec<&'static [u8]> = Vec::with_capacity(4);
    if opts.include_uppercase {
        pool.extend_from_slice(UPPER);
        required.push(UPPER);
    }
    if opts.include_lowercase {
        pool.extend_from_slice(LOWER);
        required.push(LOWER);
    }
    if opts.include_digits {
        pool.extend_from_slice(DIGITS);
        required.push(DIGITS);
    }
    if opts.include_symbols {
        pool.extend_from_slice(SYMBOLS);
        required.push(SYMBOLS);
    }

    let mut result: Vec<u8> = Vec::with_capacity(opts.length);

    if opts.length >= required.len() {
        for class in &required {
            let max_idx = i64::try_from(class.len().saturating_sub(1)).unwrap_or(0);
            let idx = usize::try_from(random_int(0, max_idx)?).unwrap_or(0);
            result.push(class[idx]);
        }
    }
    while result.len() < opts.length {
        let max_idx = i64::try_from(pool.len().saturating_sub(1)).unwrap_or(0);
        let idx = usize::try_from(random_int(0, max_idx)?).unwrap_or(0);
        result.push(pool[idx]);
    }

    for i in (1..result.len()).rev() {
        let max_i = i64::try_from(i).unwrap_or(0);
        let j = usize::try_from(random_int(0, max_i)?).unwrap_or(0);
        result.swap(i, j);
    }

    String::from_utf8(result).map_err(|_| CryptoFailure::Random("BNCrypto.RandomPassword"))
}
