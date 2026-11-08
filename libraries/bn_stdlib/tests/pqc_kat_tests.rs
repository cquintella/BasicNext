// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Known-answer tests for the post-quantum pair, against NIST ACVP vectors
// (tests/vectors/pqc_kat.txt names the files and their SHA-256). These are
// the functions both backends call, so a pass here holds for bni and bnc.
//
// ML-KEM-768 (FIPS 203): the encapsulation key derived from the seed d || z
// matches the vector byte for byte; decapsulation of a modified ciphertext
// returns the implicit-rejection key J(z || c) = SHAKE256(z || c)[0..32]
// (FIPS 203, Algorithm 18) instead of an error, and never panics.
// ML-DSA-65 (FIPS 204): the verifying key derived from the seed xi matches the
// vector; the vector's modified-message signature does not verify.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_stdlib::crypto::{
    ml_dsa_keypair, ml_dsa_sign, ml_dsa_verify, ml_kem_decapsulate, ml_kem_encapsulate,
    ml_kem_keypair,
};
use sha3::digest::{ExtendableOutput, Update, XofReader};

const VECTORS: &str = include_str!("vectors/pqc_kat.txt");
/// Bytes of an ML-KEM shared secret and of the implicit-rejection key.
const SHARED_SECRET_BYTES: usize = 32;

/// One vector line: its kind, its test-case id and its named hex fields.
struct Vector<'a> {
    kind: &'a str,
    tc_id: &'a str,
    fields: Vec<(&'a str, &'a str)>,
}

impl Vector<'_> {
    fn hex(&self, name: &str) -> Vec<u8> {
        let (_, value) = self
            .fields
            .iter()
            .find(|(field, _)| *field == name)
            .unwrap_or_else(|| panic!("tcId {}: no field {name}", self.tc_id));
        decode_hex(value)
    }

    fn text(&self, name: &str) -> &str {
        self.fields
            .iter()
            .find(|(field, _)| *field == name)
            .map_or_else(
                || panic!("tcId {}: no field {name}", self.tc_id),
                |(_, value)| *value,
            )
    }
}

fn vectors(kind: &str) -> Vec<Vector<'static>> {
    let found: Vec<Vector<'static>> = VECTORS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut parts = line.split(' ');
            let kind = parts.next().expect("kind");
            let tc_id = parts.next().expect("tcId");
            let fields = parts
                .map(|part| part.split_once('=').expect("field=value"))
                .collect();
            Vector {
                kind,
                tc_id,
                fields,
            }
        })
        .filter(|vector| vector.kind == kind)
        .collect();
    assert!(!found.is_empty(), "no {kind} vectors");
    found
}

fn decode_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digit"))
        .collect()
}

#[test]
fn ml_kem_768_key_generation_matches_nist_vectors() {
    for vector in vectors("mlkem768-keygen") {
        let seed = [vector.hex("d"), vector.hex("z")].concat();
        let (encapsulation_key, _) = ml_kem_keypair(&seed).expect("key pair");
        assert_eq!(encapsulation_key, vector.hex("ek"), "tcId {}", vector.tc_id);
    }
}

#[test]
fn ml_kem_768_decapsulation_rejects_implicitly() {
    for vector in vectors("mlkem768-keygen") {
        let z = vector.hex("z");
        let seed = [vector.hex("d"), z.clone()].concat();
        let (encapsulation_key, _) = ml_kem_keypair(&seed).expect("key pair");
        let (ciphertext, shared) = ml_kem_encapsulate(&encapsulation_key).expect("encapsulate");
        assert_eq!(
            ml_kem_decapsulate(&seed, &ciphertext).expect("decapsulate"),
            shared,
            "tcId {}: the honest ciphertext",
            vector.tc_id
        );

        let mut modified = ciphertext.clone();
        modified[0] ^= 1;
        let rejected = ml_kem_decapsulate(&seed, &modified).expect("no error on a bad ciphertext");
        let mut expected = [0_u8; SHARED_SECRET_BYTES];
        let mut shake = sha3::Shake256::default();
        shake.update(&z);
        shake.update(&modified);
        shake.finalize_xof().read(&mut expected);
        assert_eq!(rejected, expected, "tcId {}: J(z || c)", vector.tc_id);
        assert_ne!(rejected, shared, "tcId {}", vector.tc_id);
    }
}

#[test]
fn ml_dsa_65_key_generation_matches_nist_vectors() {
    for vector in vectors("mldsa65-keygen") {
        let (verifying_key, _) = ml_dsa_keypair(&vector.hex("seed")).expect("key pair");
        assert_eq!(verifying_key, vector.hex("pk"), "tcId {}", vector.tc_id);
    }
}

#[test]
fn ml_dsa_65_verification_follows_nist_vectors() {
    for vector in vectors("mldsa65-sigver") {
        let expected = vector.text("passed") == "true";
        let verified = ml_dsa_verify(
            &vector.hex("pk"),
            &vector.hex("message"),
            &vector.hex("signature"),
        );
        assert_eq!(verified, expected, "tcId {}", vector.tc_id);
    }
    // A signature made from a NIST key-generation seed verifies under the
    // NIST verifying key of that seed.
    let keygen = &vectors("mldsa65-keygen")[0];
    let message = b"post-quantum";
    let signature = ml_dsa_sign(&keygen.hex("seed"), message).expect("sign");
    assert!(ml_dsa_verify(&keygen.hex("pk"), message, &signature));
    assert!(!ml_dsa_verify(
        &keygen.hex("pk"),
        b"post-quantun",
        &signature
    ));
}
