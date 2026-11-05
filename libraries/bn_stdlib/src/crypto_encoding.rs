// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` Encoding and Checksums: Base64, `Base64Url`, PEM, strict UTF-8 text conversion,
//! Concat, and non-cryptographic checksums (CRC32, `CRC32c`, Adler32, `XxHash64`).

use std::ffi::c_char;

use crate::crypto_abi::{BN_CRYPTO_INVALID_ARGUMENT, BN_CRYPTO_OK, failed, store, with_buffers};
use crate::crypto_csprng::{decode_base64url, encode_base64url_unpadded};
use crate::crypto_error::CryptoFailure;
use crate::{c_str, c_string};

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes `bytes` as standard Base64 string with `=` padding (RFC 4648 §4).
#[must_use]
pub fn bn_base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let (chunks, rem) = bytes.as_chunks::<3>();
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(BASE64_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(BASE64_CHARS[(b2 & 0x3f) as usize] as char);
    }
    match rem.len() {
        1 => {
            let b0 = rem[0];
            out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64_CHARS[((b0 & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let b0 = rem[0];
            let b1 = rem[1];
            out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(BASE64_CHARS[((b1 & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Base64 sextets per quantum, and the most padding characters a quantum ends with.
const BASE64_QUANTUM: usize = 4;
const BASE64_MAX_PADDING: usize = 2;

/// The value of one standard Base64 digit, or `None`.
const fn base64_digit(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes a standard Base64 string (RFC 4648 §4). Line breaks and spaces are
/// skipped (PEM bodies); padding is optional, but when present it completes
/// the last quantum and is at most two `=`. A length that cannot hold a whole
/// byte and non-zero trailing bits (a non-canonical encoding) are refused.
#[allow(clippy::cast_possible_truncation)]
pub fn bn_base64_decode(input: &str) -> Result<Vec<u8>, CryptoFailure> {
    let text: Vec<u8> = input
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'\r' | b'\n' | b'\t'))
        .collect();
    let digits = text
        .iter()
        .rposition(|&b| b != b'=')
        .map_or(0, |last| last + 1);
    let padding = text.len() - digits;
    let padded_length_is_whole = padding == 0 || text.len().is_multiple_of(BASE64_QUANTUM);
    if padding > BASE64_MAX_PADDING || !padded_length_is_whole || digits % BASE64_QUANTUM == 1 {
        return Err(CryptoFailure::Base64);
    }
    let mut out = Vec::with_capacity(digits * 3 / BASE64_QUANTUM);
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in &text[..digits] {
        let value = base64_digit(b).ok_or(CryptoFailure::Base64)?;
        buf = (buf << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    if buf & ((1 << bits) - 1) != 0 {
        return Err(CryptoFailure::Base64);
    }
    Ok(out)
}

/// Encodes `bytes` as URL-safe Base64 without padding (RFC 4648 §5).
#[must_use]
pub fn bn_base64url_encode(bytes: &[u8]) -> String {
    encode_base64url_unpadded(bytes)
}

/// Decodes a URL-safe Base64 string (RFC 4648 §5), ignoring trailing padding.
pub fn bn_base64url_decode(input: &str) -> Result<Vec<u8>, CryptoFailure> {
    decode_base64url(input).ok_or(CryptoFailure::Base64Url)
}

/// Encodes `der` bytes into a standard PEM container with 64-column wrapping.
#[must_use]
pub fn bn_pem_encode(label: &str, der: &[u8]) -> String {
    let b64 = bn_base64_encode(der);
    let mut pem = format!("-----BEGIN {label}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        pem.push('\n');
    }
    pem.push_str("-----END ");
    pem.push_str(label);
    pem.push_str("-----\n");
    pem
}

/// Decodes a PEM container, extracting the Base64 payload between BEGIN and END lines.
pub fn bn_pem_decode(pem: &str) -> Result<Vec<u8>, CryptoFailure> {
    let lines: Vec<&str> = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 {
        return Err(CryptoFailure::Pem);
    }
    let first = lines[0];
    let last = lines[lines.len() - 1];
    if !first.starts_with("-----BEGIN ") || !first.ends_with("-----") {
        return Err(CryptoFailure::Pem);
    }
    if !last.starts_with("-----END ") || !last.ends_with("-----") {
        return Err(CryptoFailure::Pem);
    }
    let b64_content = lines[1..lines.len() - 1].concat();
    bn_base64_decode(&b64_content).map_err(|_| CryptoFailure::Pem)
}

/// Strict UTF-8 text conversion from bytes.
pub fn bn_bytes_to_text(bytes: &[u8]) -> Result<String, CryptoFailure> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| CryptoFailure::Utf8)
}

/// Concatenates two byte slices into a new `Vec<u8>`.
#[must_use]
pub fn bn_bytes_concat(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    out.extend_from_slice(a);
    out.extend_from_slice(b);
    out
}

#[allow(clippy::cast_possible_truncation)]
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Computes IEEE 802.3 CRC-32 (polynomial `0xEDB88320`).
#[must_use]
pub fn bn_crc32(data: &[u8]) -> u32 {
    !crc_step(&CRC32_TABLE, CRC_INIT, data)
}

/// Initial register of both CRC-32 variants (all ones).
const CRC_INIT: u32 = 0xFFFF_FFFF;

/// Feeds `data` into a reflected table-driven CRC-32 register; the final
/// checksum is the complement of the register.
fn crc_step(table: &[u32; 256], mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        let idx = ((crc ^ u32::from(b)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[idx];
    }
    crc
}

#[allow(clippy::cast_possible_truncation)]
const CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x82F6_3B78;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Computes Castagnoli CRC-32C (polynomial `0x82F63B78`).
#[must_use]
pub fn bn_crc32c(data: &[u8]) -> u32 {
    !crc_step(&CRC32C_TABLE, CRC_INIT, data)
}

/// Computes Adler-32 checksum.
#[must_use]
pub fn bn_adler32(data: &[u8]) -> u32 {
    let (a, b) = adler_step(ADLER_INIT, data);
    (b << 16) | a
}

/// Initial `(a, b)` sums of Adler-32.
const ADLER_INIT: (u32, u32) = (1, 0);

/// Feeds `data` into the Adler-32 sums `(a, b)`.
fn adler_step((mut a, mut b): (u32, u32), data: &[u8]) -> (u32, u32) {
    const MOD_ADLER: u32 = 65521;
    for &byte in data {
        a = (a + u32::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (a, b)
}

const PRIME64_1: u64 = 0x9E37_79B1_85EB_CA87;
const PRIME64_2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const PRIME64_3: u64 = 0x1656_67B1_9E37_79F9;
const PRIME64_4: u64 = 0x85EB_CA77_C2B2_AE63;
const PRIME64_5: u64 = 0x27D4_EB2F_1656_67C5;

/// Computes 64-bit xxHash checksum with seed 0.
#[must_use]
pub fn bn_xxhash64(data: &[u8]) -> u64 {
    let len = data.len();
    let mut h64: u64;

    if len >= 32 {
        let mut v1 = PRIME64_1.wrapping_add(PRIME64_2);
        let mut v2 = PRIME64_2;
        let mut v3 = 0u64;
        let mut v4 = 0u64.wrapping_sub(PRIME64_1);

        let (chunks, _) = data[..len - (len % 32)].as_chunks::<32>();
        for chunk in chunks {
            let k1 = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let k2 = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
            let k3 = u64::from_le_bytes(chunk[16..24].try_into().unwrap());
            let k4 = u64::from_le_bytes(chunk[24..32].try_into().unwrap());

            v1 = v1
                .wrapping_add(k1.wrapping_mul(PRIME64_2))
                .rotate_left(31)
                .wrapping_mul(PRIME64_1);
            v2 = v2
                .wrapping_add(k2.wrapping_mul(PRIME64_2))
                .rotate_left(31)
                .wrapping_mul(PRIME64_1);
            v3 = v3
                .wrapping_add(k3.wrapping_mul(PRIME64_2))
                .rotate_left(31)
                .wrapping_mul(PRIME64_1);
            v4 = v4
                .wrapping_add(k4.wrapping_mul(PRIME64_2))
                .rotate_left(31)
                .wrapping_mul(PRIME64_1);
        }

        h64 = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));

        let merge = |mut acc: u64, val: u64| {
            acc ^= val
                .wrapping_mul(PRIME64_2)
                .rotate_left(31)
                .wrapping_mul(PRIME64_1);
            acc.wrapping_mul(PRIME64_1).wrapping_add(PRIME64_4)
        };
        h64 = merge(h64, v1);
        h64 = merge(h64, v2);
        h64 = merge(h64, v3);
        h64 = merge(h64, v4);
    } else {
        h64 = PRIME64_5;
    }

    h64 = h64.wrapping_add(len as u64);

    let mut rem = &data[len - (len % 32)..];
    while rem.len() >= 8 {
        let k1 = u64::from_le_bytes(rem[0..8].try_into().unwrap());
        h64 ^= k1
            .wrapping_mul(PRIME64_2)
            .rotate_left(31)
            .wrapping_mul(PRIME64_1);
        h64 = h64
            .rotate_left(27)
            .wrapping_mul(PRIME64_1)
            .wrapping_add(PRIME64_4);
        rem = &rem[8..];
    }
    if rem.len() >= 4 {
        let k1 = u64::from_le_bytes([rem[0], rem[1], rem[2], rem[3], 0, 0, 0, 0]);
        h64 ^= k1.wrapping_mul(PRIME64_1);
        h64 = h64
            .rotate_left(23)
            .wrapping_mul(PRIME64_2)
            .wrapping_add(PRIME64_3);
        rem = &rem[4..];
    }
    for &b in rem {
        h64 ^= u64::from(b).wrapping_mul(PRIME64_5);
        h64 = h64.rotate_left(11).wrapping_mul(PRIME64_1);
    }

    h64 ^= h64 >> 33;
    h64 = h64.wrapping_mul(PRIME64_2);
    h64 ^= h64 >> 29;
    h64 = h64.wrapping_mul(PRIME64_3);
    h64 ^= h64 >> 32;

    h64
}

/// Bytes read from the file per step of `bn_verify_file_checksum`.
const CHECKSUM_CHUNK: usize = 64 * 1024;

/// A running checksum of the file a specifier names.
enum FileChecksum {
    Sha256(Box<crate::crypto_hash::Hasher>),
    Crc32(u32),
    Crc32c(u32),
    Adler32((u32, u32)),
}

impl FileChecksum {
    /// The checksum for a specifier scheme (`sha256`, `crc32`, `crc32c`,
    /// `adler32`, any case); `None` for any other scheme.
    fn for_scheme(scheme: &str) -> Option<Self> {
        match scheme.to_ascii_lowercase().as_str() {
            "sha256" => crate::crypto_hash::Hasher::new(crate::crypto_hash::HASH_SHA256)
                .ok()
                .map(|hasher| Self::Sha256(Box::new(hasher))),
            "crc32" => Some(Self::Crc32(CRC_INIT)),
            "crc32c" => Some(Self::Crc32c(CRC_INIT)),
            "adler32" => Some(Self::Adler32(ADLER_INIT)),
            _ => None,
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(hasher) => hasher.update(data),
            Self::Crc32(crc) => *crc = crc_step(&CRC32_TABLE, *crc, data),
            Self::Crc32c(crc) => *crc = crc_step(&CRC32C_TABLE, *crc, data),
            Self::Adler32(sums) => *sums = adler_step(*sums, data),
        }
    }

    /// Whether the finished checksum equals `expected` (hex, any case). The
    /// SHA-256 comparison is constant-time.
    fn matches(self, expected: &str) -> bool {
        let expected = expected.to_ascii_lowercase();
        match self {
            Self::Sha256(hasher) => subtle::ConstantTimeEq::ct_eq(
                crate::crypto::encode_hex(&hasher.finish()).as_bytes(),
                expected.as_bytes(),
            )
            .into(),
            Self::Crc32(crc) | Self::Crc32c(crc) => format!("{:08x}", !crc) == expected,
            Self::Adler32((a, b)) => format!("{:08x}", (b << 16) | a) == expected,
        }
    }
}

/// Verifies a file against an algorithm-tagged specifier (`"sha256:<hex>"`,
/// `"crc32:<hex>"`, `"crc32c:<hex>"`, `"adler32:<hex>"`). The file is opened
/// through `fs`, the execution policy, and read in chunks. `FALSE` for an
/// unknown scheme, a path the policy refuses, a read error or a mismatch.
#[must_use]
pub fn bn_verify_file_checksum(fs: &crate::FsPolicy, path: &str, expected: &str) -> bool {
    use std::io::Read as _;
    let Some((scheme, expected_val)) = expected.split_once(':') else {
        return false;
    };
    let Some(mut checksum) = FileChecksum::for_scheme(scheme) else {
        return false;
    };
    let Ok(mut file) = fs.open(std::path::Path::new(path), crate::secure_fs::OpenMode::Read) else {
        return false;
    };
    let mut buffer = vec![0_u8; CHECKSUM_CHUNK];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => checksum.update(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    checksum.matches(expected_val)
}

// ---------------------------------------------------------------------------
// C ABI Functions
// ---------------------------------------------------------------------------

/// Runs `f` on the live buffer `handle`. A released or unknown handle is a
/// `USE_AFTER_RELEASE` failure that ends the process, as `bni` reports it;
/// never a made-up result.
fn on_live_buffer<T>(handle: u64, f: impl FnOnce(&[u8]) -> T) -> T {
    with_buffers(|bufs| bufs.get(&handle).map(|bytes| f(bytes)))
        .unwrap_or_else(|| crate::arc_abi::abort_use_after_release())
}

/// C ABI: `ToBase64(handle) -> STRING`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_to_base64(handle: u64) -> *mut c_char {
    c_string(&on_live_buffer(handle, bn_base64_encode))
}

/// C ABI: `FromBase64(text) -> Bytes OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_from_base64(text: *const c_char, out_handle: *mut i64) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(text_str) = c_str(text) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match bn_base64_decode(text_str) {
        Ok(bytes) => {
            let h = store(bytes);
            let handle_i64 = i64::try_from(h).unwrap_or(i64::MAX);
            // SAFETY: `out_handle` was checked non-null above and points to the
            // caller's result slot.
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `ToBase64Url(handle) -> STRING`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_to_base64url(handle: u64) -> *mut c_char {
    c_string(&on_live_buffer(handle, bn_base64url_encode))
}

/// C ABI: `FromBase64Url(text) -> Bytes OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_from_base64url(
    text: *const c_char,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(text_str) = c_str(text) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match bn_base64url_decode(text_str) {
        Ok(bytes) => {
            let h = store(bytes);
            let handle_i64 = i64::try_from(h).unwrap_or(i64::MAX);
            // SAFETY: `out_handle` was checked non-null above and points to the
            // caller's result slot.
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Bytes.ToText(handle) -> STRING OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_to_text(handle: u64, out_str: *mut *mut c_char) -> i32 {
    if out_str.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(bytes) = with_buffers(|bufs| bufs.get(&handle).cloned()) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match bn_bytes_to_text(&bytes) {
        Ok(s) => {
            // SAFETY: `out_str` was checked non-null above and points to the
            // caller's result slot.
            unsafe { *out_str = c_string(&s) };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Concat(a, b) -> Bytes`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_bytes_concat(
    a_handle: u64,
    b_handle: u64,
    out_handle: *mut i64,
) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(a) = with_buffers(|bufs| bufs.get(&a_handle).cloned()) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let Some(b) = with_buffers(|bufs| bufs.get(&b_handle).cloned()) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    let combined = bn_bytes_concat(&a, &b);
    let h = store(combined);
    let handle_i64 = i64::try_from(h).unwrap_or(i64::MAX);
    // SAFETY: `out_handle` was checked non-null above and points to the
    // caller's result slot.
    unsafe { *out_handle = handle_i64 };
    BN_CRYPTO_OK
}

/// C ABI: `ToPem(label, handle) -> STRING`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_to_pem(label: *const c_char, handle: u64) -> *mut c_char {
    let Some(label_str) = c_str(label) else {
        return c_string("");
    };
    c_string(&on_live_buffer(handle, |bytes| {
        bn_pem_encode(label_str, bytes)
    }))
}

/// C ABI: `FromPem(pem) -> Bytes OR Error`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_from_pem(pem: *const c_char, out_handle: *mut i64) -> i32 {
    if out_handle.is_null() {
        return BN_CRYPTO_INVALID_ARGUMENT;
    }
    let Some(pem_str) = c_str(pem) else {
        return BN_CRYPTO_INVALID_ARGUMENT;
    };
    match bn_pem_decode(pem_str) {
        Ok(bytes) => {
            let h = store(bytes);
            let handle_i64 = i64::try_from(h).unwrap_or(i64::MAX);
            // SAFETY: `out_handle` was checked non-null above and points to the
            // caller's result slot.
            unsafe { *out_handle = handle_i64 };
            BN_CRYPTO_OK
        }
        Err(err) => failed(&err),
    }
}

/// C ABI: `Checksum.Crc32(handle) -> INTEGER`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_crc32(handle: u64) -> i64 {
    on_live_buffer(handle, |bytes| i64::from(bn_crc32(bytes)))
}

/// C ABI: `Checksum.Crc32c(handle) -> INTEGER`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_crc32c(handle: u64) -> i64 {
    on_live_buffer(handle, |bytes| i64::from(bn_crc32c(bytes)))
}

/// C ABI: `Checksum.Adler32(handle) -> INTEGER`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_adler32(handle: u64) -> i64 {
    on_live_buffer(handle, |bytes| i64::from(bn_adler32(bytes)))
}

/// C ABI: `Checksum.XxHash64(handle) -> INTEGER`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_xxhash64(handle: u64) -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    on_live_buffer(handle, |bytes| bn_xxhash64(bytes) as i64)
}

/// C ABI: `Checksum.VerifyFileChecksum(path, expected) -> 1 | 0`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_verify_file_checksum(
    path: *const c_char,
    expected: *const c_char,
) -> i32 {
    let Some(path_str) = c_str(path) else {
        return 0;
    };
    let Some(expected_str) = c_str(expected) else {
        return 0;
    };
    i32::from(crate::policy::with_fs(|fs| {
        bn_verify_file_checksum(fs, path_str, expected_str)
    }))
}
