// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! OS-backed CSPRNG utilities for Basic Next cryptography:
//! unbiased random integers via rejection sampling, URL-safe base64 tokens,
//! RFC 4122 `UUIDv4`, RFC 9562 `UUIDv7`, and structured password generation.

use std::ffi::c_char;

use crate::c_string;
use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed, store};
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

/// Decodes a URL-safe base64 string (RFC 4648 §5), ignoring trailing padding `=`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    let s = input.trim_end_matches('=');
    let mut out = Vec::with_capacity((s.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
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
#[must_use]
pub fn random_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("system entropy failed");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(&bytes)
}

static UUID_V7_STATE: std::sync::Mutex<(u64, u16)> = std::sync::Mutex::new((0, 0));

/// Generates a time-ordered RFC 9562 UUID version 7 string.
///
/// Guaranteed to be monotonically increasing even when generated within the
/// same millisecond, using an RFC 9562 monotonic counter.
#[must_use]
pub fn uuid_v7() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("system entropy failed");
    let millis = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_millis(),
    )
    .unwrap_or(0);

    let (ms, counter) = {
        let mut state = UUID_V7_STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if millis > state.0 {
            let initial_counter = u16::from_be_bytes([bytes[6] & 0x0f, bytes[7]]) & 0x07ff;
            *state = (millis, initial_counter);
        } else {
            // Same millisecond or backward clock step: increment counter monotonically
            state.1 = state.1.saturating_add(1);
            if state.1 > 0x0fff {
                // When 12-bit counter rolls over within the same millisecond,
                // advance the simulated timestamp by 1 ms (RFC 9562 §6.2)
                state.0 = state.0.saturating_add(1);
                state.1 = 0;
            }
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
    format_uuid(&bytes)
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
const TOTAL_POOL_CAPACITY: usize = UPPER.len() + LOWER.len() + DIGITS.len() + SYMBOLS.len();

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
    let mut pool: Vec<u8> = Vec::with_capacity(TOTAL_POOL_CAPACITY);
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

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_bytes(length: i64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let bytes = match random_bytes(i128::from(length)) {
        Ok(b) => b,
        Err(err) => return failed(&err),
    };
    let handle = store(bytes);
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out = handle };
    BN_CRYPTO_OK
}

/// Struct layout offsets for `PasswordGenOptions` (16-byte header + fields).
const PASSWORD_OPTS_HEADER_BYTES: usize = 16;
const PASSWORD_OPTS_LENGTH_OFFSET: usize = PASSWORD_OPTS_HEADER_BYTES;
const PASSWORD_OPTS_UPPER_OFFSET: usize = PASSWORD_OPTS_HEADER_BYTES + 4;
const PASSWORD_OPTS_LOWER_OFFSET: usize = PASSWORD_OPTS_HEADER_BYTES + 5;
const PASSWORD_OPTS_DIGITS_OFFSET: usize = PASSWORD_OPTS_HEADER_BYTES + 6;
const PASSWORD_OPTS_SYMBOLS_OFFSET: usize = PASSWORD_OPTS_HEADER_BYTES + 7;

/// Generates an unbiased random integer in `[min, max]`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_int(min: i64, max: i64, out: *mut i64) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    // SAFETY: out was checked non-null and points at an allocated i64 slot.
    unsafe { *out = 0 };
    match random_int(min, max) {
        Ok(val) => {
            // SAFETY: out is valid and writable.
            unsafe { *out = val };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// Generates a URL-safe base64 string from `n_bytes` random bytes without padding.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_token(n_bytes: i64, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    // SAFETY: out was checked non-null and points at an allocated pointer slot.
    unsafe { *out = std::ptr::null_mut() };
    match random_token(n_bytes) {
        Ok(tok) => {
            // SAFETY: out is valid and writable.
            unsafe { *out = c_string(&tok) };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// Generates a canonical RFC 4122 UUID version 4 string.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_uuid_v4() -> *mut c_char {
    let u = random_uuid_v4();
    c_string(&u)
}

/// Generates a time-ordered RFC 9562 UUID version 7 string.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_uuid_v7() -> *mut c_char {
    let u = uuid_v7();
    c_string(&u)
}

/// Generates a secure random password with requested length and character classes.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_random_password(opts_ptr: *const u8, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    // SAFETY: out was checked non-null and points at an allocated pointer slot.
    unsafe { *out = std::ptr::null_mut() };
    if opts_ptr.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    // `opts_ptr` was checked non-null and is the `PasswordGenOptions` record the
    // backend laid out: a 16-byte header, the `INT32` length, four boolean bytes.
    let flag = |offset| {
        // SAFETY: the record has a byte at each `PASSWORD_OPTS_*_OFFSET`.
        let byte = unsafe { crate::crypto_policy::read_option_byte(opts_ptr, offset) };
        byte != 0
    };
    // SAFETY: as above; the length is the `INT32` at `PASSWORD_OPTS_LENGTH_OFFSET`.
    let length =
        unsafe { crate::crypto_policy::read_option_i32(opts_ptr, PASSWORD_OPTS_LENGTH_OFFSET) };
    let upper = flag(PASSWORD_OPTS_UPPER_OFFSET);
    let lower = flag(PASSWORD_OPTS_LOWER_OFFSET);
    let digits = flag(PASSWORD_OPTS_DIGITS_OFFSET);
    let symbols = flag(PASSWORD_OPTS_SYMBOLS_OFFSET);
    let opts = PasswordGenOptions {
        length: usize::try_from(length).unwrap_or(0),
        include_uppercase: upper,
        include_lowercase: lower,
        include_digits: digits,
        include_symbols: symbols,
    };
    match random_password(opts) {
        Ok(pwd) => {
            // SAFETY: out was checked non-null and points at a writable pointer slot.
            unsafe { *out = c_string(&pwd) };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}
