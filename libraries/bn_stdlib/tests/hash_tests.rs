// Basic Next crypto hash and streaming tests
// Checks SHA-2, SHA-3, BLAKE2b, BLAKE3, Hasher streaming, and file streaming.

#![allow(unsafe_code)]
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
    Alg, HASH_BLAKE2B, HASH_BLAKE3, HASH_MD5, HASH_SHA1, HASH_SHA3_256, HASH_SHA3_512, HASH_SHA256,
    HASH_SHA384, HASH_SHA512, HASH_SHA512_256, Hasher, bn_hash, bn_hash_file,
};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_hash_file, bn_rt_crypto_hash_handle, bn_rt_crypto_hasher_finish,
    bn_rt_crypto_hasher_new, bn_rt_crypto_hasher_release, bn_rt_crypto_hasher_update_handle,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use std::ffi::c_char;
use std::fs;

struct BnString(*mut c_char);

impl BnString {
    fn new(bytes: &[u8]) -> Self {
        let pointer = c_string_from_bytes(bytes);
        assert!(!pointer.is_null(), "allocate STRING");
        Self(pointer)
    }

    fn as_ptr(&self) -> *const c_char {
        self.0
    }
}

impl Drop for BnString {
    fn drop(&mut self) {
        bn_rt_str_release(self.0);
    }
}

/// A live `Bytes` handle holding `bytes`, made through the reachable C ABI.
fn buffer(bytes: &[u8]) -> u64 {
    let hex =
        bn_stdlib::string::c_string_from_bytes(bn_stdlib::crypto::encode_hex(bytes).as_bytes());
    let mut handle = 0_u64;
    let rc = bn_stdlib::crypto_abi::bn_rt_crypto_bytes_from_hex(hex, &raw mut handle);
    bn_stdlib::string::bn_rt_str_release(hex);
    assert_eq!(rc, 0, "the call succeeds");
    handle
}

#[test]
fn test_hash_algorithms_vectors() {
    let input = b"Hello Basic Next Hash!";

    // SHA-256
    assert_eq!(Alg::SHA256, HASH_SHA256);
    let sha256 = bn_hash(input, HASH_SHA256).expect("sha256");
    assert_eq!(sha256.len(), 32);

    // SHA-384
    let sha384 = bn_hash(input, HASH_SHA384).expect("sha384");
    assert_eq!(sha384.len(), 48);

    // SHA-512
    let sha512 = bn_hash(input, HASH_SHA512).expect("sha512");
    assert_eq!(sha512.len(), 64);

    // SHA-512/256
    let sha512_256 = bn_hash(input, HASH_SHA512_256).expect("sha512/256");
    assert_eq!(sha512_256.len(), 32);

    // SHA3-256
    let sha3_256 = bn_hash(input, HASH_SHA3_256).expect("sha3-256");
    assert_eq!(sha3_256.len(), 32);

    // SHA3-512
    let sha3_512 = bn_hash(input, HASH_SHA3_512).expect("sha3-512");
    assert_eq!(sha3_512.len(), 64);

    // BLAKE2b
    let blake2b = bn_hash(input, HASH_BLAKE2B).expect("blake2b");
    assert_eq!(blake2b.len(), 64);

    // BLAKE3
    let blake3 = bn_hash(input, HASH_BLAKE3).expect("blake3");
    assert_eq!(blake3.len(), 32);

    // Legacy algorithms fail without allowLegacy
    // MD5 and SHA-1 are reserved: refused whatever the policy, with
    // LEGACY_DISABLED; an id that names no algorithm is UNSUPPORTED_ALGORITHM.
    for legacy in [HASH_MD5, HASH_SHA1] {
        assert_eq!(
            bn_hash(input, legacy).expect_err("legacy hash").code(),
            bn_types::error_codes::crypto::LEGACY_DISABLED
        );
    }
    assert_eq!(
        bn_hash(input, 999).expect_err("unknown hash").code(),
        bn_types::error_codes::crypto::UNSUPPORTED_ALGORITHM
    );

    // Unknown algorithm code fails
    assert!(bn_hash(input, 999).is_err());
}

#[test]
fn test_hasher_streaming() {
    let chunk1 = b"Hello ";
    let chunk2 = b"Basic Next Hash!";
    let full = b"Hello Basic Next Hash!";

    let expected = bn_hash(full, HASH_SHA256).expect("expected sha256");

    let mut hasher = Hasher::new(HASH_SHA256).expect("hasher new");
    hasher.update(chunk1);
    hasher.update(chunk2);
    let streamed = hasher.finish();

    assert_eq!(streamed, expected);

    // Also test streaming with BLAKE3
    let expected_b3 = bn_hash(full, HASH_BLAKE3).expect("expected blake3");
    let mut hasher_b3 = Hasher::new(HASH_BLAKE3).expect("hasher new blake3");
    hasher_b3.update(chunk1);
    hasher_b3.update(chunk2);
    let streamed_b3 = hasher_b3.finish();

    assert_eq!(streamed_b3, expected_b3);
}

#[test]
fn test_hash_file_streaming() {
    let payload = vec![0x37u8; 150_000]; // 150 KiB (crosses 64 KiB segments)
    let temp_dir = std::env::temp_dir().join(format!("bn-hash-test-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let file_path = temp_dir.join("payload.dat");
    fs::write(&file_path, &payload).expect("write file");

    let expected = bn_hash(&payload, HASH_SHA256).expect("hash payload");
    let file_hashed = bn_hash_file(
        &bn_stdlib::FsPolicy::unrestricted(),
        &file_path,
        HASH_SHA256,
    )
    .expect("hash file");

    assert_eq!(file_hashed, expected);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_c_abi_hasher() {
    let mut handle = 0i64;
    let rc = bn_rt_crypto_hasher_new(HASH_SHA256, &raw mut handle);
    assert_eq!(rc, 0);
    assert_ne!(handle, 0);

    let data = b"c abi hash streaming test";
    let data_handle = i64::try_from(buffer(data)).expect("handle");
    let rc_up = bn_rt_crypto_hasher_update_handle(handle, data_handle);
    assert_eq!(rc_up, 0);

    let mut out_handle = 0i64;
    let rc_fin = bn_rt_crypto_hasher_finish(handle, &raw mut out_handle);
    assert_eq!(rc_fin, 0);
    assert_ne!(out_handle, 0);

    let _ = bn_rt_crypto_hasher_release(handle);

    let digest_handle = bn_rt_crypto_hash_handle(data_handle, HASH_SHA256);
    assert_ne!(digest_handle, 0);

    let bn_str = BnString::new(b"test_hash_input.tmp");
    let mut file_digest_handle = 0i64;
    let _ = bn_rt_crypto_hash_file(bn_str.as_ptr(), HASH_SHA256, &raw mut file_digest_handle);
}
