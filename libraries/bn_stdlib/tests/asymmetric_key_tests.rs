// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Basic Next asymmetric key tests
// Checks PublicKey, PrivateKey, KeyPair generation, KeyId (SPKI SHA-256),
// algorithm reporting, public key derivation from private key, and zeroization.

#![allow(unsafe_code)]

use bn_stdlib::crypto::{
    bn_ed25519_keypair_generate, bn_keypair_generate_internal, bn_private_key_algorithm,
    bn_private_key_id, bn_private_key_public_key, bn_private_key_zeroize, bn_public_key_algorithm,
    bn_public_key_id,
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
    assert_eq!(pub_alg, 1); // Alg.ED25519 = 1
    assert_eq!(priv_alg, 1);

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
    let rc = bn_rt_crypto_keypair_generate(1, &raw mut pub_handle, &raw mut priv_handle);
    assert_eq!(rc, 0);
    assert_ne!(pub_handle, 0);
    assert_ne!(priv_handle, 0);

    let pub_id_ptr = bn_rt_crypto_public_key_id(pub_handle);
    assert!(!pub_id_ptr.is_null());
    let pub_id = unsafe { CStr::from_ptr(pub_id_ptr) }.to_str().unwrap();
    assert_eq!(pub_id.len(), 64);

    let priv_id_ptr = bn_rt_crypto_private_key_id(priv_handle);
    assert!(!priv_id_ptr.is_null());
    let priv_id = unsafe { CStr::from_ptr(priv_id_ptr) }.to_str().unwrap();
    assert_eq!(priv_id, pub_id);

    assert_eq!(bn_rt_crypto_public_key_algorithm(pub_handle), 1);
    assert_eq!(bn_rt_crypto_private_key_algorithm(priv_handle), 1);

    let derived_pub = bn_rt_crypto_private_key_public_key(priv_handle);
    assert_ne!(derived_pub, 0);
    let derived_id_ptr = bn_rt_crypto_public_key_id(derived_pub);
    assert!(!derived_id_ptr.is_null());
    let derived_id = unsafe { CStr::from_ptr(derived_id_ptr) }.to_str().unwrap();
    assert_eq!(derived_id, pub_id);

    assert_eq!(bn_rt_crypto_private_key_zeroize(priv_handle), 0);

    bn_rt_str_release(pub_id_ptr);
    bn_rt_str_release(priv_id_ptr);
    bn_rt_str_release(derived_id_ptr);

    assert_eq!(bn_rt_crypto_public_key_release(pub_handle), 0);
    assert_eq!(bn_rt_crypto_private_key_release(priv_handle), 0);
    assert_eq!(bn_rt_crypto_public_key_release(derived_pub), 0);
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
    assert!(bn_keypair_generate_internal(999).is_err());
}
