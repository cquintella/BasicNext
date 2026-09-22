// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! SHA-256 and SHA-512 digests behind the C ABI, so the interpreter
//! (`bn_lib_crypto`) and compiled binaries (`bn_llvm`) share one
//! implementation rather than two that can drift.

use std::ffi::c_char;

use sha2::{Digest, Sha256, Sha512};

use crate::{c_str, c_string};

/// SHA-256 of `data` as 64 lowercase hex characters.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// SHA-512 of `data` as 128 lowercase hex characters.
#[must_use]
pub fn sha512_hex(data: &[u8]) -> String {
    format!("{:x}", Sha512::digest(data))
}

/// Lowercase hex rendering of `bytes`.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Bytes decoded from an even-length hex string, or `None` when the length is
/// odd or a character is not a hex digit. Accepts either case.
#[must_use]
pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

/// SHA-256 over the UTF-8 bytes of `text`. Returns a freshly allocated
/// NUL-terminated string owned by the caller, freed like other owned rt
/// strings. A null or non-UTF-8 argument yields `""`, which is never a
/// valid digest.
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_sha256(text: *const c_char) -> *mut c_char {
    let Some(text) = c_str(text) else {
        return c_string("");
    };
    c_string(&sha256_hex(text.as_bytes()))
}

/// SHA-512 over the UTF-8 bytes of `text`. Ownership and the null/non-UTF-8
/// behaviour match [`bn_rt_crypto_sha256`].
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_crypto_sha512(text: *const c_char) -> *mut c_char {
    let Some(text) = c_str(text) else {
        return c_string("");
    };
    c_string(&sha512_hex(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{sha256_hex, sha512_hex};

    // Inputs are the published FIPS 180-4 example messages; the expected
    // digests were computed by Python hashlib — an implementation independent
    // of `sha2` — and are mirrored in tests/fixtures/crypto/sha-vectors.json.
    // A failure here is an implementation bug, never a reason to edit the
    // expectation.
    const FIPS_448_BIT: &str = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";

    #[test]
    fn sha256_matches_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(FIPS_448_BIT.as_bytes()),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha512_matches_fips_vectors() {
        assert_eq!(
            sha512_hex(b""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            sha512_hex(b"abc"),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(
            sha512_hex(FIPS_448_BIT.as_bytes()),
            "204a8fc6dda82f0a0ced7beb8e08a41657c16ef468b228a8279be331a703c335\
             96fd15c13b1b07f9aa1d3bea57789ca031ad85c7a71dd70354ec631238ca3445"
        );
    }

    #[test]
    fn hex_round_trips_and_rejects_malformed_input() {
        use super::{decode_hex, encode_hex};
        assert_eq!(encode_hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(decode_hex("000fff"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(decode_hex("000FFF"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(decode_hex(""), Some(Vec::new()));
        // Odd length is impossible as a byte string — the defect that made the
        // first attempt's fabricated fixtures detectable.
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn digest_widths_are_fixed() {
        assert_eq!(sha256_hex(b"any length input").len(), 64);
        assert_eq!(sha512_hex(b"any length input").len(), 128);
    }
}
