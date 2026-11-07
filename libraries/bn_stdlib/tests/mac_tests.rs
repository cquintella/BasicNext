// Basic Next crypto MAC and streaming tests
// Checks HMAC-SHA-256, HMAC-SHA-384, HMAC-SHA-512, BLAKE3_KEYED,
// constant-time verification, and MacStream streaming.

#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::similar_names,
    clippy::borrow_as_ptr
)]

use bn_stdlib::crypto::{
    Alg, MAC_BLAKE3_KEYED, MAC_HMAC_SHA256, MAC_HMAC_SHA384, MAC_HMAC_SHA512, MacStream, bn_mac,
    bn_mac_verify,
};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_bytes_length, bn_rt_crypto_bytes_release, bn_rt_crypto_mac_handle,
    bn_rt_crypto_mac_stream_finish, bn_rt_crypto_mac_stream_new, bn_rt_crypto_mac_stream_new_with,
    bn_rt_crypto_mac_stream_release, bn_rt_crypto_mac_stream_update_handle,
    bn_rt_crypto_mac_stream_verify, bn_rt_crypto_mac_verify_handle, bn_rt_crypto_mac_with,
};

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
fn test_mac_algorithms_and_verification() {
    let key = [0x42u8; 32];
    let data = b"Hello Basic Next MAC!";

    // HMAC-SHA256
    let tag256 = bn_mac(&key, data, MAC_HMAC_SHA256).expect("hmac-sha256");
    assert_eq!(tag256.len(), 32);
    assert!(bn_mac_verify(&key, data, &tag256, MAC_HMAC_SHA256));

    // Tampered tag fails
    let mut bad_tag = tag256.clone();
    bad_tag[0] ^= 0x01;
    assert!(!bn_mac_verify(&key, data, &bad_tag, MAC_HMAC_SHA256));

    // HMAC-SHA384
    let tag384 = bn_mac(&key, data, MAC_HMAC_SHA384).expect("hmac-sha384");
    assert_eq!(tag384.len(), 48);
    assert!(bn_mac_verify(&key, data, &tag384, MAC_HMAC_SHA384));

    // HMAC-SHA512
    let tag512 = bn_mac(&key, data, MAC_HMAC_SHA512).expect("hmac-sha512");
    assert_eq!(tag512.len(), 64);
    assert!(bn_mac_verify(&key, data, &tag512, MAC_HMAC_SHA512));

    // BLAKE3_KEYED
    let tag_b3 = bn_mac(&key, data, MAC_BLAKE3_KEYED).expect("blake3-keyed");
    assert_eq!(tag_b3.len(), 32);
    assert!(bn_mac_verify(&key, data, &tag_b3, MAC_BLAKE3_KEYED));

    // Alg constants check
    assert_eq!(Alg::HMAC_SHA256, MAC_HMAC_SHA256);
    assert_eq!(Alg::HMAC_SHA384, MAC_HMAC_SHA384);
    assert_eq!(Alg::HMAC_SHA512, MAC_HMAC_SHA512);
    assert_eq!(Alg::BLAKE3_KEYED, MAC_BLAKE3_KEYED);
}

#[test]
fn test_mac_stream_streaming() {
    let key = [0x5au8; 32];
    let chunk1 = b"Hello ";
    let chunk2 = b"Basic Next MAC!";
    let full = b"Hello Basic Next MAC!";

    let expected = bn_mac(&key, full, MAC_HMAC_SHA256).expect("expected mac");

    let mut stream = MacStream::new(&key, MAC_HMAC_SHA256).expect("stream new");
    stream.update(chunk1);
    stream.update(chunk2);
    let tag = stream.finish();

    assert_eq!(tag, expected);

    // Verify stream verify method
    let mut stream_v = MacStream::new(&key, MAC_HMAC_SHA256).expect("stream new verify");
    stream_v.update(full);
    assert!(stream_v.verify(&expected));
}

#[test]
fn test_c_abi_mac_stream() {
    let key_bytes = [0x77u8; 32];
    let key_handle = buffer(&key_bytes);
    assert_ne!(key_handle, 0);

    let data = b"C ABI MAC streaming test";
    let mut stream_handle = 0i64;
    let rc_new = bn_rt_crypto_mac_stream_new(key_handle, MAC_HMAC_SHA256, &raw mut stream_handle);
    assert_eq!(rc_new, 0);
    assert_ne!(stream_handle, 0);

    let data_handle = buffer(data);
    let rc_up = bn_rt_crypto_mac_stream_update_handle(stream_handle, data_handle.cast_signed());
    assert_eq!(rc_up, 0);

    let mut tag_handle = 0i64;
    let rc_fin = bn_rt_crypto_mac_stream_finish(stream_handle, &raw mut tag_handle);
    assert_eq!(rc_fin, 0);
    assert_ne!(tag_handle, 0);

    assert_eq!(bn_rt_crypto_bytes_length(tag_handle.cast_unsigned()), 32);

    // One-shot MAC C ABI
    let oneshot_handle = bn_rt_crypto_mac_handle(key_handle, data_handle);
    assert_ne!(oneshot_handle, 0);

    // Verify C ABI
    let match_rc =
        bn_rt_crypto_mac_verify_handle(key_handle, data_handle, oneshot_handle.cast_unsigned());
    assert_eq!(match_rc, 1);

    // Stream verify C ABI
    let mut stream_v = 0i64;
    let _ = bn_rt_crypto_mac_stream_new(key_handle, MAC_HMAC_SHA256, &raw mut stream_v);
    let _ = bn_rt_crypto_mac_stream_update_handle(stream_v, data_handle.cast_signed());
    let v_match = bn_rt_crypto_mac_stream_verify(stream_v, tag_handle);
    assert_eq!(v_match, 1);

    // MacWith and MacStream.NewWith C ABI
    let mut opts_buf = [0u8; 24];
    opts_buf[16..20].copy_from_slice(&MAC_HMAC_SHA256.to_le_bytes());
    let mut mac_with_handle = 0i64;
    let rc_with = bn_rt_crypto_mac_with(
        key_handle,
        data_handle,
        opts_buf.as_ptr(),
        &raw mut mac_with_handle,
    );
    assert_eq!(rc_with, 0);
    assert_ne!(mac_with_handle, 0);

    let mut stream_with_handle = 0i64;
    let rc_stream_with = bn_rt_crypto_mac_stream_new_with(
        key_handle,
        opts_buf.as_ptr(),
        &raw mut stream_with_handle,
    );
    assert_eq!(rc_stream_with, 0);
    assert_ne!(stream_with_handle, 0);

    let _ = bn_rt_crypto_mac_stream_release(stream_with_handle);
    let _ = bn_rt_crypto_bytes_release(data_handle);
    let _ = bn_rt_crypto_bytes_release(mac_with_handle.cast_unsigned());

    let _ = bn_rt_crypto_mac_stream_release(stream_handle);
    let _ = bn_rt_crypto_bytes_release(key_handle);
    let _ = bn_rt_crypto_bytes_release(tag_handle.cast_unsigned());
    let _ = bn_rt_crypto_bytes_release(oneshot_handle.cast_unsigned());
}
