// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Digests, hex codecs and AEAD behind the C ABI, so the interpreter
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

/// AEAD algorithms `BNCrypto` exposes. Both take a 32-byte key and a 12-byte
/// nonce and append a 16-byte authentication tag to the ciphertext.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Aead {
    Aes256Gcm,
    ChaCha20Poly1305,
}

/// Encrypts and authenticates. `None` when the key or nonce has the wrong
/// length; there is no partial result.
#[must_use]
pub fn seal(
    algorithm: Aead,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Option<Vec<u8>> {
    use aes_gcm::aead::{Aead as _, KeyInit as _, Payload};
    if key.len() != 32 || nonce.len() != 12 {
        return None;
    }
    let payload = Payload {
        msg: plaintext,
        aad,
    };
    match algorithm {
        Aead::Aes256Gcm => aes_gcm::Aes256Gcm::new_from_slice(key)
            .ok()?
            .encrypt(aes_gcm::Nonce::from_slice(nonce), payload)
            .ok(),
        Aead::ChaCha20Poly1305 => chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
            .ok()?
            .encrypt(chacha20poly1305::Nonce::from_slice(nonce), payload)
            .ok(),
    }
}

/// Verifies and decrypts. `None` when the key or nonce has the wrong length or
/// the tag does not verify — a tampered message yields no plaintext at all,
/// never a partial or unauthenticated one.
#[must_use]
pub fn open(
    algorithm: Aead,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Option<Vec<u8>> {
    use aes_gcm::aead::{Aead as _, KeyInit as _, Payload};
    if key.len() != 32 || nonce.len() != 12 {
        return None;
    }
    let payload = Payload {
        msg: ciphertext,
        aad,
    };
    match algorithm {
        Aead::Aes256Gcm => aes_gcm::Aes256Gcm::new_from_slice(key)
            .ok()?
            .decrypt(aes_gcm::Nonce::from_slice(nonce), payload)
            .ok(),
        Aead::ChaCha20Poly1305 => chacha20poly1305::ChaCha20Poly1305::new_from_slice(key)
            .ok()?
            .decrypt(chacha20poly1305::Nonce::from_slice(nonce), payload)
            .ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_hex, sha256_hex, sha512_hex};

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
        use super::decode_hex;
        assert_eq!(encode_hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(decode_hex("000fff"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(decode_hex("000FFF"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(decode_hex(""), Some(Vec::new()));
        // Odd length is impossible as a byte string — the defect that made the
        // first attempt's fabricated fixtures detectable.
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    // Key 00..1f and nonce 00..0b; ciphertexts produced by python-cryptography
    // (OpenSSL), independent of the crates under test, and mirrored in
    // tests/fixtures/crypto/aead-vectors.json.
    fn aead_key() -> Vec<u8> {
        (0..32).collect()
    }

    fn aead_nonce() -> Vec<u8> {
        (0..12).collect()
    }

    #[test]
    fn aes_gcm_matches_independent_vectors() {
        use super::{Aead, seal};
        let (key, nonce) = (aead_key(), aead_nonce());
        assert_eq!(
            encode_hex(&seal(Aead::Aes256Gcm, &key, &nonce, b"", b"").expect("seal empty")),
            "f4c2db1dc38805a37b92171c5d0a81cc"
        );
        assert_eq!(
            encode_hex(&seal(Aead::Aes256Gcm, &key, &nonce, b"abc", b"").expect("seal abc")),
            "2660b5539677125a571f571ada456e85769a43"
        );
        assert_eq!(
            encode_hex(&seal(Aead::Aes256Gcm, &key, &nonce, b"abc", b"hdr").expect("seal aad")),
            "2660b569dff070e184bce4347ab72c6ec46088"
        );
    }

    #[test]
    fn chacha20_poly1305_matches_independent_vectors() {
        use super::{Aead, seal};
        let (key, nonce) = (aead_key(), aead_nonce());
        assert_eq!(
            encode_hex(&seal(Aead::ChaCha20Poly1305, &key, &nonce, b"", b"").expect("seal empty")),
            "295a498b8841a1c5f55d4d606f731159"
        );
        assert_eq!(
            encode_hex(&seal(Aead::ChaCha20Poly1305, &key, &nonce, b"abc", b"").expect("seal abc")),
            "e8996bdbe97d036a9b815bccbdf1a8e87ec37f"
        );
        assert_eq!(
            encode_hex(
                &seal(Aead::ChaCha20Poly1305, &key, &nonce, b"abc", b"hdr").expect("seal aad")
            ),
            "e8996b3a03c9656f5001909f4819e887d5daa2"
        );
    }

    #[test]
    fn open_round_trips_and_fails_closed() {
        use super::{Aead, open, seal};
        let (key, nonce) = (aead_key(), aead_nonce());
        for algorithm in [Aead::Aes256Gcm, Aead::ChaCha20Poly1305] {
            let sealed = seal(algorithm, &key, &nonce, b"abc", b"hdr").expect("seal");
            assert_eq!(
                open(algorithm, &key, &nonce, &sealed, b"hdr").as_deref(),
                Some(&b"abc"[..])
            );
            // A tampered byte, a changed AAD, or a different key must yield no
            // plaintext at all — never a partial or unauthenticated one.
            let mut tampered = sealed.clone();
            tampered[0] ^= 1;
            assert_eq!(open(algorithm, &key, &nonce, &tampered, b"hdr"), None);
            assert_eq!(open(algorithm, &key, &nonce, &sealed, b"other"), None);
            let mut other_key = key.clone();
            other_key[0] ^= 1;
            assert_eq!(open(algorithm, &other_key, &nonce, &sealed, b"hdr"), None);
        }
    }

    #[test]
    fn aead_rejects_wrong_key_and_nonce_lengths() {
        use super::{Aead, seal};
        let (key, nonce) = (aead_key(), aead_nonce());
        assert_eq!(seal(Aead::Aes256Gcm, &key[..16], &nonce, b"abc", b""), None);
        assert_eq!(seal(Aead::Aes256Gcm, &key, &nonce[..8], b"abc", b""), None);
    }

    #[test]
    fn digest_widths_are_fixed() {
        assert_eq!(sha256_hex(b"any length input").len(), 64);
        assert_eq!(sha512_hex(b"any length input").len(), 128);
    }
}
