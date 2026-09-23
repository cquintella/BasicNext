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

/// HMAC-SHA-256 of `data` under `key` (RFC 2104). The key may be any length.
///
/// # Panics
///
/// Never in practice: `SimpleHmac` accepts a key of any length, so the
/// construction cannot fail.
#[must_use]
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Mac, SimpleHmac};
    let mut mac = SimpleHmac::<Sha256>::new_from_slice(key).expect("SimpleHmac accepts any key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time comparison of a MAC against `expected`. Returns `false` for a
/// length mismatch. The comparison must not short-circuit: an attacker who can
/// time it would otherwise recover the tag byte by byte.
#[must_use]
pub fn hmac_verify(key: &[u8], data: &[u8], tag: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    let expected = hmac_sha256(key, data);
    expected.len() == tag.len() && bool::from(expected.ct_eq(tag))
}

/// Cost and optional inputs for [`argon2id`]. `secret` and `associated_data`
/// are the RFC 9106 optional key and AD inputs and may be empty.
#[derive(Clone, Copy, Debug)]
pub struct Argon2Params<'a> {
    pub secret: &'a [u8],
    pub associated_data: &'a [u8],
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub tag_length: usize,
}

/// Argon2id (RFC 9106). `None` when a parameter is outside the algorithm's
/// accepted range, never a weaker fallback.
#[must_use]
pub fn argon2id(password: &[u8], salt: &[u8], params: Argon2Params<'_>) -> Option<Vec<u8>> {
    use argon2::{Algorithm, Argon2, AssociatedData, ParamsBuilder, Version};
    let Argon2Params {
        secret,
        associated_data,
        memory_kib,
        iterations,
        parallelism,
        tag_length,
    } = params;
    let mut builder = ParamsBuilder::new();
    builder
        .m_cost(memory_kib)
        .t_cost(iterations)
        .p_cost(parallelism)
        .output_len(tag_length);
    if !associated_data.is_empty() {
        builder.data(AssociatedData::try_from(associated_data).ok()?);
    }
    let params = builder.build().ok()?;
    let hasher = if secret.is_empty() {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    } else {
        Argon2::new_with_secret(secret, Algorithm::Argon2id, Version::V0x13, params).ok()?
    };
    let mut out = vec![0_u8; tag_length];
    hasher.hash_password_into(password, salt, &mut out).ok()?;
    Some(out)
}

/// Ed25519 public key for a 32-byte seed (RFC 8032). `None` when the seed is
/// not exactly 32 bytes.
#[must_use]
pub fn ed25519_public_key(seed: &[u8]) -> Option<Vec<u8>> {
    let seed: [u8; 32] = seed.try_into().ok()?;
    Some(
        ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes()
            .to_vec(),
    )
}

/// Ed25519 signature over `message` for a 32-byte seed. Signing is
/// deterministic, so the same seed and message always produce the same 64-byte
/// signature.
#[must_use]
pub fn ed25519_sign(seed: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    use ed25519_dalek::Signer as _;
    let seed: [u8; 32] = seed.try_into().ok()?;
    Some(
        ed25519_dalek::SigningKey::from_bytes(&seed)
            .sign(message)
            .to_bytes()
            .to_vec(),
    )
}

/// Verifies an Ed25519 signature. `false` for a malformed key or signature, a
/// tampered message, or a signature from another key — never an error the
/// caller might mistake for success.
#[must_use]
pub fn ed25519_verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use ed25519_dalek::Verifier as _;
    let (Ok(public_key), Ok(signature)) = (
        <[u8; 32]>::try_from(public_key),
        <[u8; 64]>::try_from(signature),
    ) else {
        return false;
    };
    ed25519_dalek::VerifyingKey::from_bytes(&public_key)
        .map(|key| key.verify(message, &ed25519_dalek::Signature::from_bytes(&signature)))
        .is_ok_and(|verified| verified.is_ok())
}

/// ECDSA P-256 public key in SEC1 uncompressed form (65 bytes) for a 32-byte
/// private scalar.
#[must_use]
pub fn p256_public_key(private_key: &[u8]) -> Option<Vec<u8>> {
    let key = p256::ecdsa::SigningKey::from_slice(private_key).ok()?;
    Some(
        key.verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec(),
    )
}

/// ECDSA P-256 signature over SHA-256 of `message`, as raw `r || s` (64 bytes).
/// The nonce is derived per RFC 6979, so signing is deterministic.
#[must_use]
pub fn p256_sign(private_key: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    use p256::ecdsa::signature::Signer as _;
    let key = p256::ecdsa::SigningKey::from_slice(private_key).ok()?;
    let signature: p256::ecdsa::Signature = key.sign(message);
    Some(signature.to_bytes().to_vec())
}

/// Verifies a raw `r || s` ECDSA P-256 signature against a SEC1 public key.
#[must_use]
pub fn p256_verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use p256::ecdsa::signature::Verifier as _;
    let (Ok(key), Ok(signature)) = (
        p256::ecdsa::VerifyingKey::from_sec1_bytes(public_key),
        p256::ecdsa::Signature::from_slice(signature),
    ) else {
        return false;
    };
    key.verify(message, &signature).is_ok()
}

/// ML-KEM-768 key pair from a 64-byte seed (FIPS 203 `d || z`). Returns
/// `(encapsulation key, decapsulation seed)`.
#[must_use]
pub fn ml_kem_keypair(seed: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    use ml_kem::{DecapsulationKey, KeyExport, MlKem768, Seed};
    let seed: [u8; 64] = seed.try_into().ok()?;
    let decapsulation = DecapsulationKey::<MlKem768>::from_seed(Seed::from(seed));
    Some((
        decapsulation.encapsulation_key().to_bytes().to_vec(),
        seed.to_vec(),
    ))
}

/// Encapsulates against an ML-KEM-768 encapsulation key with fresh randomness,
/// returning `(ciphertext, shared secret)`. Deterministic encapsulation exists
/// in the crate but is deliberately not exposed: reusing the randomness even
/// once is a catastrophic failure, so the choice is not offered.
#[must_use]
pub fn ml_kem_encapsulate(encapsulation_key: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    use ml_kem::{EncapsulationKey, MlKem768, kem::Encapsulate};
    let key = EncapsulationKey::<MlKem768>::new(encapsulation_key.try_into().ok()?).ok()?;
    let (ciphertext, shared) = key.encapsulate();
    Some((ciphertext.to_vec(), shared.to_vec()))
}

/// Recovers the ML-KEM-768 shared secret from a ciphertext, given the seed the
/// decapsulation key was built from.
#[must_use]
pub fn ml_kem_decapsulate(seed: &[u8], ciphertext: &[u8]) -> Option<Vec<u8>> {
    use ml_kem::{Decapsulate, DecapsulationKey, MlKem768, Seed};
    let seed: [u8; 64] = seed.try_into().ok()?;
    let key = DecapsulationKey::<MlKem768>::from_seed(Seed::from(seed));
    let shared = key.decapsulate(ciphertext.try_into().ok()?);
    Some(shared.to_vec())
}

/// ML-DSA-65 key pair from a 32-byte seed (FIPS 204). Returns
/// `(verifying key, seed)`; the seed is the signing key's portable form.
#[must_use]
pub fn ml_dsa_keypair(seed: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    use ml_dsa::{Keypair, MlDsa65, SigningKey};
    let seed: [u8; 32] = seed.try_into().ok()?;
    let signing = SigningKey::<MlDsa65>::from_seed(&seed.into());
    Some((signing.verifying_key().encode().to_vec(), seed.to_vec()))
}

/// ML-DSA-65 signature over `message`.
#[must_use]
pub fn ml_dsa_sign(seed: &[u8], message: &[u8]) -> Option<Vec<u8>> {
    use ml_dsa::{MlDsa65, SignatureEncoding, Signer, SigningKey};
    let seed: [u8; 32] = seed.try_into().ok()?;
    let signing = SigningKey::<MlDsa65>::from_seed(&seed.into());
    Some(signing.sign(message).to_bytes().to_vec())
}

/// Verifies an ML-DSA-65 signature. `false` for a malformed key or signature,
/// a tampered message, or a signature from another key.
#[must_use]
pub fn ml_dsa_verify(verifying_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use ml_dsa::{MlDsa65, Verifier, VerifyingKey};
    let Ok(key_bytes) = verifying_key.try_into() else {
        return false;
    };
    let key = VerifyingKey::<MlDsa65>::decode(key_bytes);
    let Ok(signature) = signature.try_into() else {
        return false;
    };
    key.verify(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{decode_hex, encode_hex, sha256_hex, sha512_hex};

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
    fn hmac_matches_an_independent_vector_and_verifies_in_constant_time() {
        use super::{hmac_sha256, hmac_verify};
        // Produced by Python's hmac module, independent of the `hmac` crate.
        let key = b"0123456789abcdef0123456789abcdef";
        let tag = hmac_sha256(key, b"abc");
        assert_eq!(
            encode_hex(&tag),
            "a60c859a6827c5ea576a48d8d368672fbfe4667c6a927428284a0cb3859cc1d6"
        );
        assert!(hmac_verify(key, b"abc", &tag));
        // Tampered tag, tampered message, wrong key, wrong length: all false.
        let mut bad = tag.clone();
        bad[0] ^= 1;
        assert!(!hmac_verify(key, b"abc", &bad));
        assert!(!hmac_verify(key, b"abd", &tag));
        assert!(!hmac_verify(b"another key", b"abc", &tag));
        assert!(!hmac_verify(key, b"abc", &tag[..31]));
    }

    #[test]
    fn argon2id_matches_the_rfc_9106_test_vector() {
        use super::{Argon2Params, argon2id};
        // RFC 9106 test vector, taken from the RFC itself: password 0x01*32,
        // salt 0x02*16, secret 0x03*8, associated data 0x04*12, m=32 KiB, t=3,
        // p=4, 32-byte tag.
        let tag = argon2id(
            &[1; 32],
            &[2; 16],
            Argon2Params {
                secret: &[3; 8],
                associated_data: &[4; 12],
                memory_kib: 32,
                iterations: 3,
                parallelism: 4,
                tag_length: 32,
            },
        )
        .expect("RFC 9106 parameters are in range");
        assert_eq!(
            encode_hex(&tag),
            "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"
        );
    }

    #[test]
    fn argon2id_is_salt_and_parameter_sensitive() {
        use super::{Argon2Params, argon2id};
        let params = |memory_kib, iterations| Argon2Params {
            secret: &[],
            associated_data: &[],
            memory_kib,
            iterations,
            parallelism: 4,
            tag_length: 32,
        };
        let base = argon2id(b"password", &[2; 16], params(32, 3)).expect("base");
        let other_salt = argon2id(b"password", &[3; 16], params(32, 3)).expect("salt");
        let other_time = argon2id(b"password", &[2; 16], params(32, 4)).expect("time");
        assert_ne!(base, other_salt);
        assert_ne!(base, other_time);
        // Below the algorithm's minimum memory there is no weaker fallback.
        assert_eq!(argon2id(b"password", &[2; 16], params(1, 3)), None);
    }

    #[test]
    fn ed25519_matches_an_independent_implementation() {
        use super::{ed25519_public_key, ed25519_sign, ed25519_verify};
        // Key and signature produced by python-cryptography, independent of
        // ed25519-dalek. Ed25519 signing is deterministic (RFC 8032), so the
        // bytes must match exactly.
        let seed: Vec<u8> = (0..32).collect();
        let public = ed25519_public_key(&seed).expect("32-byte seed");
        assert_eq!(
            encode_hex(&public),
            "03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8"
        );
        let signature = ed25519_sign(&seed, b"abc").expect("32-byte seed");
        assert_eq!(
            encode_hex(&signature),
            "cc46d62d3754f41754b27b6ea2cb2c272bafa7a5a1f6062bd060f414e50caaea\
             c2da66ad39cef4424a90236ea907b7d8057e3443dc5abfc9986967ee7213a407"
        );
        assert!(ed25519_verify(&public, b"abc", &signature));
        // Tampered message, tampered signature and a malformed key all reject.
        assert!(!ed25519_verify(&public, b"abd", &signature));
        let mut bad = signature.clone();
        bad[0] ^= 1;
        assert!(!ed25519_verify(&public, b"abc", &bad));
        assert!(!ed25519_verify(&public[..31], b"abc", &signature));
    }

    #[test]
    fn p256_interoperates_with_an_independent_implementation() {
        use super::{p256_public_key, p256_sign, p256_verify};
        // Private scalar 01..20; the public key and signature come from
        // python-cryptography. ECDSA nonces differ between implementations, so
        // this asserts interoperability rather than identical signature bytes:
        // our verifier must accept their signature.
        let private: Vec<u8> = (1..=32).collect();
        let public = p256_public_key(&private).expect("valid scalar");
        assert_eq!(
            encode_hex(&public),
            "04515c3d6eb9e396b904d3feca7f54fdcd0cc1e997bf375dca515ad0a6c3b4035f\
             4536be3a50f318fbf9a5475902a221502bef0d57e08c53b2cc0a56f17d9f9354"
        );
        let theirs = decode_hex(
            "aa5c01b39b8aeae0e3c09013856254b4488bd5c4e72c7ad894fb25058ab7959c\
             2f9e1bfc6390530a6ec19d60cebd0292c9214c48c096c27bae461023f7270f7b",
        )
        .expect("vector is valid hex");
        assert!(p256_verify(&public, b"abc", &theirs));
        assert!(!p256_verify(&public, b"abd", &theirs));

        // Our own signature round-trips and is RFC 6979 deterministic.
        let ours = p256_sign(&private, b"abc").expect("valid scalar");
        assert_eq!(ours.len(), 64);
        assert_eq!(ours, p256_sign(&private, b"abc").expect("deterministic"));
        assert!(p256_verify(&public, b"abc", &ours));
    }

    #[test]
    fn post_quantum_round_trips_and_reports_its_sizes() {
        use super::{
            ml_dsa_keypair, ml_dsa_sign, ml_dsa_verify, ml_kem_decapsulate, ml_kem_encapsulate,
            ml_kem_keypair,
        };
        // ML-KEM-768: the shared secret both sides derive must agree, and key
        // generation from a seed must be reproducible.
        let kem_seed: Vec<u8> = (0..64).collect();
        let (encapsulation, decapsulation) = ml_kem_keypair(&kem_seed).expect("64-byte seed");
        assert_eq!(
            ml_kem_keypair(&kem_seed).expect("deterministic").0,
            encapsulation
        );
        let (ciphertext, sent) = ml_kem_encapsulate(&encapsulation).expect("valid key");
        let received = ml_kem_decapsulate(&decapsulation, &ciphertext).expect("valid ciphertext");
        assert_eq!(sent, received);
        // Encapsulation must not be deterministic: fresh randomness each time.
        let (other_ciphertext, _) = ml_kem_encapsulate(&encapsulation).expect("valid key");
        assert_ne!(ciphertext, other_ciphertext);

        // ML-DSA-65: sign/verify round-trips and rejects tampering.
        let dsa_seed: Vec<u8> = (0..32).collect();
        let (verifying, signing) = ml_dsa_keypair(&dsa_seed).expect("32-byte seed");
        let signature = ml_dsa_sign(&signing, b"abc").expect("valid seed");
        assert!(ml_dsa_verify(&verifying, b"abc", &signature));
        assert!(!ml_dsa_verify(&verifying, b"abd", &signature));
        let mut tampered = signature.clone();
        tampered[0] ^= 1;
        assert!(!ml_dsa_verify(&verifying, b"abc", &tampered));

        // Sizes published in FIPS 203 (ML-KEM-768) and FIPS 204 (ML-DSA-65).
        // An implementation that produced different ones would not be the
        // standardised parameter set.
        assert_eq!(encapsulation.len(), 1184);
        assert_eq!(ciphertext.len(), 1088);
        assert_eq!(sent.len(), 32);
        assert_eq!(verifying.len(), 1952);
        assert_eq!(signature.len(), 3309);
    }

    #[test]
    fn digest_widths_are_fixed() {
        assert_eq!(sha256_hex(b"any length input").len(), 64);
        assert_eq!(sha512_hex(b"any length input").len(), 128);
    }
}
