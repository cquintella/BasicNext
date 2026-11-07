// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Basic Next asymmetric key tests
// Checks PublicKey, PrivateKey, KeyPair generation, KeyId (SPKI SHA-256),
// algorithm reporting, public key derivation from private key, and zeroization.

#![allow(unsafe_code)]

use bn_stdlib::crypto::{
    PEM_FORMAT_PKCS8, bn_ed25519_keypair_generate, bn_export_private_pem,
    bn_export_private_pem_unencrypted, bn_export_public_pem, bn_keypair_generate_internal,
    bn_load_private_pem, bn_load_public_pem, bn_private_key_algorithm, bn_private_key_id,
    bn_private_key_public_key, bn_private_key_zeroize, bn_public_key_algorithm, bn_public_key_id,
    bn_sign, bn_sign_file, bn_verify, bn_verify_file, ed25519_public_key, p256_public_key,
};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_keypair_generate, bn_rt_crypto_private_key_algorithm, bn_rt_crypto_private_key_id,
    bn_rt_crypto_private_key_public_key, bn_rt_crypto_private_key_release,
    bn_rt_crypto_private_key_zeroize, bn_rt_crypto_public_key_algorithm,
    bn_rt_crypto_public_key_id, bn_rt_crypto_public_key_release,
};
use bn_stdlib::string::bn_rt_str_release;
use std::ffi::CStr;

#[test]
fn test_ed25519_keypair_and_key_ids() {
    let (pub_handle, priv_handle) =
        bn_ed25519_keypair_generate().expect("generate ed25519 keypair");
    assert_ne!(pub_handle, 0);
    assert_ne!(priv_handle, 0);

    let pub_id = bn_public_key_id(pub_handle).expect("public key id");
    let priv_id = bn_private_key_id(priv_handle).expect("private key id");
    assert_eq!(pub_id.len(), 64);
    assert_eq!(pub_id, priv_id);

    let pub_alg = bn_public_key_algorithm(pub_handle).expect("public key alg");
    let priv_alg = bn_private_key_algorithm(priv_handle).expect("private key alg");
    assert_eq!(pub_alg, 30); // Alg.ED25519 = 30
    assert_eq!(priv_alg, 30);

    let derived_pub =
        bn_private_key_public_key(priv_handle).expect("derive public key from private");
    let derived_id = bn_public_key_id(derived_pub).expect("derived public key id");
    assert_eq!(derived_id, pub_id);

    assert!(bn_private_key_zeroize(priv_handle));
}

#[test]
fn test_asymmetric_key_c_abi() {
    let mut pub_handle = 0u64;
    let mut priv_handle = 0u64;
    let rc = bn_rt_crypto_keypair_generate(30, &raw mut pub_handle, &raw mut priv_handle);
    assert_eq!(rc, 0);
    assert_ne!(pub_handle, 0);
    assert_ne!(priv_handle, 0);

    let pub_id_ptr = bn_rt_crypto_public_key_id(pub_handle);
    assert!(!pub_id_ptr.is_null());
    // SAFETY: the runtime returned this NUL-terminated STRING, alive until it is released.
    let pub_id = unsafe { CStr::from_ptr(pub_id_ptr) }.to_str().unwrap();
    assert_eq!(pub_id.len(), 64);

    let priv_id_ptr = bn_rt_crypto_private_key_id(priv_handle);
    assert!(!priv_id_ptr.is_null());
    // SAFETY: the runtime returned this NUL-terminated STRING, alive until it is released.
    let priv_id = unsafe { CStr::from_ptr(priv_id_ptr) }.to_str().unwrap();
    assert_eq!(priv_id, pub_id);

    assert_eq!(bn_rt_crypto_public_key_algorithm(pub_handle), 30);
    assert_eq!(bn_rt_crypto_private_key_algorithm(priv_handle), 30);

    let derived_pub = bn_rt_crypto_private_key_public_key(priv_handle);
    assert_ne!(derived_pub, 0);
    let derived_id_ptr = bn_rt_crypto_public_key_id(derived_pub);
    assert!(!derived_id_ptr.is_null());
    // SAFETY: the runtime returned this NUL-terminated STRING, alive until it is released.
    let derived_id = unsafe { CStr::from_ptr(derived_id_ptr) }.to_str().unwrap();
    assert_eq!(derived_id, pub_id);

    assert_eq!(bn_rt_crypto_private_key_zeroize(priv_handle), 0);
    // The public half holds no secret: it stays derivable after zeroize.
    assert_eq!(bn_rt_crypto_private_key_algorithm(priv_handle), 30);
    let after_zeroize = bn_rt_crypto_private_key_public_key(priv_handle);
    assert_ne!(after_zeroize, 0);
    assert_eq!(bn_rt_crypto_public_key_release(after_zeroize), 0);

    bn_rt_str_release(pub_id_ptr);
    bn_rt_str_release(priv_id_ptr);
    bn_rt_str_release(derived_id_ptr);

    assert_eq!(bn_rt_crypto_public_key_release(pub_handle), 0);
    assert_eq!(bn_rt_crypto_private_key_release(priv_handle), 0);
    assert_eq!(bn_rt_crypto_public_key_release(derived_pub), 0);

    // An unsupported selector reports its own code natively, as in bni.
    let rc = bn_rt_crypto_keypair_generate(999, &raw mut pub_handle, &raw mut priv_handle);
    assert_ne!(rc, 0);
}

#[test]
fn test_x25519_keypair_generation() {
    let (pub_handle, priv_handle) =
        bn_keypair_generate_internal(31).expect("generate x25519 keypair");
    assert_ne!(pub_handle, 0);
    assert_ne!(priv_handle, 0);

    let pub_id = bn_public_key_id(pub_handle).expect("public key id");
    let priv_id = bn_private_key_id(priv_handle).expect("private key id");
    assert_eq!(pub_id.len(), 64);
    assert_eq!(pub_id, priv_id);

    let pub_alg = bn_public_key_algorithm(pub_handle).expect("public key alg");
    let priv_alg = bn_private_key_algorithm(priv_handle).expect("private key alg");
    assert_eq!(pub_alg, 31);
    assert_eq!(priv_alg, 31);

    let derived_pub =
        bn_private_key_public_key(priv_handle).expect("derive public key from private");
    let derived_id = bn_public_key_id(derived_pub).expect("derived public key id");
    assert_eq!(derived_id, pub_id);

    assert!(bn_private_key_zeroize(priv_handle));
}

#[test]
fn test_p256_keypair_generation() {
    let (pub_handle, priv_handle) =
        bn_keypair_generate_internal(32).expect("generate p256 keypair");
    assert_ne!(pub_handle, 0);
    assert_ne!(priv_handle, 0);

    let pub_id = bn_public_key_id(pub_handle).expect("public key id");
    let priv_id = bn_private_key_id(priv_handle).expect("private key id");
    assert_eq!(pub_id.len(), 64);
    assert_eq!(pub_id, priv_id);

    let pub_alg = bn_public_key_algorithm(pub_handle).expect("public key alg");
    let priv_alg = bn_private_key_algorithm(priv_handle).expect("private key alg");
    assert_eq!(pub_alg, 32);
    assert_eq!(priv_alg, 32);

    let derived_pub =
        bn_private_key_public_key(priv_handle).expect("derive public key from private");
    let derived_id = bn_public_key_id(derived_pub).expect("derived public key id");
    assert_eq!(derived_id, pub_id);

    assert!(bn_private_key_zeroize(priv_handle));
}

#[test]
fn test_unsupported_algorithm_keypair_generation() {
    let err = bn_keypair_generate_internal(999).expect_err("unknown algorithm");
    assert_eq!(err.code(), 4);
    // The symmetric and legacy ALG_* ids (1 AES-GCM, 4 and 5 the legacy
    // Ed25519 and P-256 keys) are not key-pair selectors.
    for algorithm in [1, 4, 5] {
        let err = bn_keypair_generate_internal(algorithm).expect_err("not a key-pair selector");
        assert_eq!(
            err.code(),
            bn_types::error_codes::crypto::UNSUPPORTED_ALGORITHM
        );
    }
}

const PASSWORD: &str = "s3cret-phrase";

fn assert_password_absent(err: &bn_stdlib::crypto_error::CryptoFailure) {
    assert!(
        !err.message().contains(PASSWORD),
        "the error message never repeats the password"
    );
    assert!(
        !err.cause().contains(PASSWORD),
        "the error cause never repeats the password"
    );
    assert!(
        !err.message().contains("not-the-password"),
        "the error message never repeats the wrong password"
    );
    assert!(
        !err.cause().contains("not-the-password"),
        "the error cause never repeats the wrong password"
    );
}

#[test]
fn detached_signatures_follow_the_key_algorithm() {
    let seed = [9_u8; 32];
    let message = b"asymmetric";
    let ed_public = ed25519_public_key(&seed).expect("ed25519 public");
    let ed_sig = bn_sign(&seed, 30, message).expect("ed25519 sign");
    // The legacy ALG_ED25519 id (4) is not a key-pair algorithm.
    assert_eq!(bn_sign(&seed, 4, message).unwrap_err().code(), 4);
    assert!(bn_verify(&ed_public, 30, message, &ed_sig));
    assert!(!bn_verify(&ed_public, 30, b"other", &ed_sig));
    assert_eq!(bn_sign(&seed, 31, message).unwrap_err().code(), 7);
    assert!(!bn_verify(&ed_public, 31, message, &ed_sig));

    let p256_seed = [1_u8; 32];
    let p256_public = p256_public_key(&p256_seed).expect("p256 public");
    let p256_sig = bn_sign(&p256_seed, 32, message).expect("p256 sign");
    assert_eq!(bn_sign(&p256_seed, 5, message).unwrap_err().code(), 4);
    assert!(bn_verify(&p256_public, 32, message, &p256_sig));
    assert!(!bn_verify(&p256_public, 32, message, &ed_sig));
}

#[test]
fn sign_file_verifies_and_a_missing_file_is_false() {
    let path = "bn-asymmetric-sign.tmp";
    std::fs::write(path, b"file-body").expect("write");
    let seed = [3_u8; 32];
    let public = ed25519_public_key(&seed).expect("public");
    let signature =
        bn_sign_file(&bn_stdlib::FsPolicy::unrestricted(), &seed, 30, path).expect("sign file");
    assert!(bn_verify_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &public,
        30,
        path,
        &signature
    ));
    assert!(!bn_verify_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &public,
        30,
        "bn-asymmetric-missing.tmp",
        &signature
    ));
    let _ = std::fs::remove_file(path);
    assert_eq!(
        bn_sign_file(
            &bn_stdlib::FsPolicy::unrestricted(),
            &seed,
            30,
            "bn-asymmetric-missing.tmp"
        )
        .unwrap_err()
        .code(),
        8
    );
}

#[test]
fn pem_format_constant_matches_the_module() {
    let module = include_str!("../../../modules/bn/BNCrypto.bn");
    assert!(module.contains("PUBLIC STATIC PKCS8 AS INT32 = 1"));
    assert!(module.contains("alg AS INT32 = 30"));
    assert_eq!(PEM_FORMAT_PKCS8, 1);
}

#[test]
fn pkcs8_export_roundtrips_and_fails_closed() {
    for algorithm in [30, 31, 32] {
        let seed = match algorithm {
            32 => vec![1_u8; 32],
            _ => vec![9_u8; 32],
        };
        let encrypted = bn_export_private_pem(&seed, algorithm, PEM_FORMAT_PKCS8, PASSWORD)
            .unwrap_or_else(|err| panic!("export {algorithm}: {err:?}"));
        assert!(encrypted.contains("ENCRYPTED PRIVATE KEY"));
        assert!(!encrypted.contains(PASSWORD));
        let loaded = bn_load_private_pem(&encrypted, Some(PASSWORD)).expect("load");
        assert_eq!(loaded.algorithm, algorithm);
        assert_eq!(loaded.seed.as_slice(), seed.as_slice());

        let wrong = bn_load_private_pem(&encrypted, Some("not-the-password")).unwrap_err();
        assert_eq!(wrong.code(), 9);
        assert_password_absent(&wrong);
        let missing = bn_load_private_pem(&encrypted, None).unwrap_err();
        assert_eq!(missing.code(), 9);

        let plain =
            bn_export_private_pem_unencrypted(&seed, algorithm, PEM_FORMAT_PKCS8).expect("plain");
        assert!(plain.contains("BEGIN PRIVATE KEY"));
        assert!(!plain.contains("ENCRYPTED"));
        let opened = bn_load_private_pem(&plain, None).expect("plain load");
        assert_eq!(opened.seed.as_slice(), seed.as_slice());
        assert_eq!(bn_load_private_pem(&plain, Some("")).unwrap_err().code(), 6);
        assert_eq!(
            bn_load_private_pem(&plain, Some(PASSWORD))
                .unwrap_err()
                .code(),
            6
        );
        assert_eq!(bn_load_public_pem(&plain).unwrap_err().code(), 6);

        let ambiguous = format!("{encrypted}\n{plain}");
        assert_eq!(
            bn_load_private_pem(&ambiguous, Some(PASSWORD))
                .unwrap_err()
                .code(),
            6
        );

        let public_pem = bn_export_public_pem(&loaded.spki);
        let public = bn_load_public_pem(&public_pem).expect("public");
        assert_eq!(public.key_id, loaded.key_id);
        assert_eq!(public.algorithm, algorithm);
    }
    assert_eq!(
        bn_export_private_pem(&[9_u8; 32], 30, 2, PASSWORD)
            .unwrap_err()
            .code(),
        1
    );
    let empty =
        bn_export_private_pem(&[9_u8; 32], 30, PEM_FORMAT_PKCS8, "").expect("empty password");
    assert!(empty.contains("ENCRYPTED PRIVATE KEY"));
}

/// The Ed25519 identity point: a small-order public key under which a
/// signature with R = identity and S = 0 verifies any message unless the
/// check is strict.
fn ed25519_identity_point() -> [u8; 32] {
    let mut point = [0_u8; 32];
    point[0] = 1;
    point
}

#[test]
fn ed25519_verify_refuses_a_forgery_under_a_small_order_key() {
    let key = ed25519_identity_point();
    let mut forged = [0_u8; 64];
    forged[..32].copy_from_slice(&key);
    assert!(!bn_verify(&key, 30, b"one message", &forged));
    assert!(!bn_verify(&key, 30, b"another message", &forged));
}

#[test]
fn load_public_key_refuses_a_small_order_ed25519_key() {
    let mut spki = bn_stdlib::crypto_asymmetric::ED25519_SPKI_HEADER.to_vec();
    spki.extend_from_slice(&ed25519_identity_point());
    let refused = bn_stdlib::crypto_asymmetric::public_entry_from_spki(&spki);
    assert_eq!(
        refused.map(|_| ()).map_err(|failure| failure.code()),
        Err(bn_types::error_codes::crypto::PARSE_ERROR)
    );
}

/// A private key entry never prints its seed, even through `Debug`.
#[test]
fn private_key_entry_debug_redacts_the_seed() {
    let seed = vec![0xAB_u8; 32];
    let entry =
        bn_stdlib::crypto_asymmetric::private_entry_from_seed(zeroize::Zeroizing::new(seed), 30)
            .expect("entry");
    let shown = format!("{entry:?}");
    assert!(!shown.contains("171"), "{shown}");
    assert!(!shown.to_lowercase().contains("abab"), "{shown}");
}

/// Ed25519 public key generated outside BN; the expected strings come from
/// `openssl pkey -pubin -outform DER | openssl dgst -<alg> -binary | base64`.
const OPENSSL_PUBLIC_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAk+xWWxvNZSixctwFqhD5ShMD7ODc71kn6ZHIVjh4eoY=\n-----END PUBLIC KEY-----\n";

#[test]
fn test_fingerprint_known_answers_and_refusals() {
    use bn_stdlib::crypto_asymmetric::bn_spki_fingerprint;
    use bn_stdlib::crypto_error::CryptoFailure;
    use bn_stdlib::crypto_hash::{HASH_MD5, HASH_SHA1, HASH_SHA256, HASH_SHA512};

    let entry = bn_load_public_pem(OPENSSL_PUBLIC_PEM).expect("load the openssl public key");
    assert_eq!(
        bn_spki_fingerprint(&entry.spki, HASH_SHA256).expect("sha256"),
        "SHA256:4fyOdzq5wz64hRbUcAvXZ2kqm4hTC2K9fYNEvUXClMg"
    );
    assert_eq!(
        bn_spki_fingerprint(&entry.spki, HASH_SHA512).expect("sha512"),
        "SHA512:rB+QpjB88c7ZhmBLWuCIEHP5cLK9UiZWREQtLTHqSrwPDtf8GwNHuo1heF6VA6l1OPtIwCT4hWXiJJsq02RcAw"
    );
    for legacy in [HASH_MD5, HASH_SHA1] {
        assert!(matches!(
            bn_spki_fingerprint(&entry.spki, legacy),
            Err(CryptoFailure::LegacyDisabled { .. })
        ));
    }
    // 11 is HMAC_SHA256, 0 and a negative id name no algorithm at all.
    for unsupported in [11, 0, -1, i32::MIN, i32::MAX] {
        assert!(
            matches!(
                bn_spki_fingerprint(&entry.spki, unsupported),
                Err(CryptoFailure::UnsupportedAlgorithm { .. })
            ),
            "{unsupported}"
        );
    }
}

#[test]
fn test_fingerprint_abi_refuses_unknown_handle_and_null_slot() {
    use bn_stdlib::crypto_sign::bn_rt_crypto_public_fingerprint;
    let mut out: *mut std::ffi::c_char = std::ptr::null_mut();
    assert_ne!(
        bn_rt_crypto_public_fingerprint(u64::MAX, 1, std::ptr::addr_of_mut!(out)),
        0
    );
    assert!(out.is_null());
    let (pub_handle, _) = bn_ed25519_keypair_generate().expect("generate ed25519 keypair");
    assert_ne!(
        bn_rt_crypto_public_fingerprint(pub_handle, 1, std::ptr::null_mut()),
        0
    );
}
