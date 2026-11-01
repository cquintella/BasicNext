// Basic Next crypto encoding and checksum tests
// Checks Base64, Base64Url, PEM, strict UTF-8 text conversion, Concat,
// CRC32, CRC32c, Adler32, XxHash64, and file checksum verification.

#![allow(unsafe_code, clippy::similar_names)]

use bn_stdlib::crypto::{
    bn_adler32, bn_base64_decode, bn_base64_encode, bn_base64url_decode, bn_base64url_encode,
    bn_bytes_concat, bn_bytes_to_text, bn_crc32, bn_crc32c, bn_pem_decode, bn_pem_encode,
    bn_verify_file_checksum, bn_xxhash64,
};
use bn_stdlib::crypto_abi::{
    bn_rt_crypto_adler32, bn_rt_crypto_bytes_concat, bn_rt_crypto_bytes_from_base64,
    bn_rt_crypto_bytes_from_base64url, bn_rt_crypto_bytes_from_raw, bn_rt_crypto_bytes_length,
    bn_rt_crypto_bytes_release, bn_rt_crypto_bytes_to_base64, bn_rt_crypto_bytes_to_base64url,
    bn_rt_crypto_bytes_to_text, bn_rt_crypto_crc32, bn_rt_crypto_crc32c, bn_rt_crypto_from_pem,
    bn_rt_crypto_to_pem, bn_rt_crypto_verify_file_checksum, bn_rt_crypto_xxhash64,
};
use bn_stdlib::string::{bn_rt_str_release, c_string_from_bytes};
use std::ffi::{CStr, c_char};
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

#[test]
fn test_base64_and_base64url() {
    let vectors = [
        (b"".as_slice(), "", ""),
        (b"f".as_slice(), "Zg==", "Zg"),
        (b"fo".as_slice(), "Zm8=", "Zm8"),
        (b"foo".as_slice(), "Zm9v", "Zm9v"),
        (b"foob".as_slice(), "Zm9vYg==", "Zm9vYg"),
        (b"fooba".as_slice(), "Zm9vYmE=", "Zm9vYmE"),
        (b"foobar".as_slice(), "Zm9vYmFy", "Zm9vYmFy"),
    ];

    for (raw, b64_expected, b64url_expected) in vectors {
        let b64 = bn_base64_encode(raw);
        assert_eq!(b64, b64_expected);
        let dec_b64 = bn_base64_decode(&b64).expect("decode base64");
        assert_eq!(dec_b64, raw);

        let b64url = bn_base64url_encode(raw);
        assert_eq!(b64url, b64url_expected);
        let dec_b64url = bn_base64url_decode(&b64url).expect("decode base64url");
        assert_eq!(dec_b64url, raw);
    }

    // Invalid base64
    assert!(bn_base64_decode("!@#$%^").is_err());
    assert!(bn_base64url_decode("!@#$%^").is_err());
}

#[test]
fn test_pem_encoding_and_decoding() {
    let der = b"mock-der-payload-bytes";
    let pem = bn_pem_encode("CERTIFICATE", der);
    assert!(pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
    assert!(pem.ends_with("-----END CERTIFICATE-----\n"));

    let decoded = bn_pem_decode(&pem).expect("decode valid pem");
    assert_eq!(decoded, der);

    // Malformed PEM
    assert!(bn_pem_decode("-----BEGIN CERTIFICATE-----\nZm9v\n").is_err());
    assert!(bn_pem_decode("not a pem").is_err());
}

#[test]
fn test_strict_utf8_to_text() {
    let valid = b"Hello Basic Next \xc3\xa9!";
    let text = bn_bytes_to_text(valid).expect("valid utf-8");
    assert_eq!(text, "Hello Basic Next é!");

    let invalid = [0xff, 0xfe, 0x12];
    assert!(bn_bytes_to_text(&invalid).is_err());
}

#[test]
fn test_bytes_concat() {
    let a = b"Part A ";
    let b = b"Part B";
    let combined = bn_bytes_concat(a, b);
    assert_eq!(combined, b"Part A Part B");
}

#[test]
fn test_non_cryptographic_checksums() {
    let input = b"123456789";

    // Standard known test vector values for "123456789"
    let crc = bn_crc32(input);
    assert_eq!(crc, 0xcbf4_3926);

    let crc_c = bn_crc32c(input);
    assert_eq!(crc_c, 0xe306_9283);

    let adler = bn_adler32(input);
    assert_eq!(adler, 0x091e_01de);

    let xx64_empty = bn_xxhash64(b"");
    assert_eq!(xx64_empty, 0xef46_db37_51d8_e999);

    let xx64_abc = bn_xxhash64(b"abc");
    assert_eq!(xx64_abc, 0x44bc_2cf5_ad77_0999);

    let xx64 = bn_xxhash64(input);
    assert_eq!(xx64, 0x8cb8_41db_40e6_ae83);
}

#[test]
fn test_verify_file_checksum() {
    let path = "test_checksum_verify.tmp";
    fs::write(path, b"123456789").expect("write test file");

    // SHA-256 of "123456789" is 15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225
    let sha256_spec = "sha256:15e2b0d3c33891ebb0f1ef609ec419420c20e320ce94c65fbc8c3312448eb225";
    assert!(bn_verify_file_checksum(path, sha256_spec));

    // CRC-32 of "123456789" is cbf43926
    let crc32_spec = "crc32:cbf43926";
    assert!(bn_verify_file_checksum(path, crc32_spec));

    // Mismatched checksum
    assert!(!bn_verify_file_checksum(path, "crc32:00000000"));

    // Unknown scheme
    assert!(!bn_verify_file_checksum(path, "unknown:cbf43926"));

    let _ = fs::remove_file(path);
}

#[test]
fn test_c_abi_encoding_and_checksums() {
    let raw = b"Hello ABI Encoding!";
    let handle = bn_rt_crypto_bytes_from_raw(raw.as_ptr(), raw.len());
    assert_ne!(handle, 0);

    // ToBase64
    let b64_ptr = bn_rt_crypto_bytes_to_base64(handle);
    assert!(!b64_ptr.is_null());
    let b64_str = unsafe { CStr::from_ptr(b64_ptr) }
        .to_str()
        .expect("valid utf-8");
    assert_eq!(b64_str, "SGVsbG8gQUJJIEVuY29kaW5nIQ==");

    // FromBase64
    let mut dec_handle = 0i64;
    let rc_from_b64 = bn_rt_crypto_bytes_from_base64(b64_ptr, &raw mut dec_handle);
    assert_eq!(rc_from_b64, 0);
    assert_ne!(dec_handle, 0);
    assert_eq!(
        bn_rt_crypto_bytes_length(dec_handle.cast_unsigned()),
        i64::try_from(raw.len()).unwrap()
    );

    // ToBase64Url
    let b64url_ptr = bn_rt_crypto_bytes_to_base64url(handle);
    assert!(!b64url_ptr.is_null());
    let b64url_str = unsafe { CStr::from_ptr(b64url_ptr) }
        .to_str()
        .expect("valid utf-8");
    assert_eq!(b64url_str, "SGVsbG8gQUJJIEVuY29kaW5nIQ");

    // FromBase64Url
    let mut dec_url_handle = 0i64;
    let rc_from_url = bn_rt_crypto_bytes_from_base64url(b64url_ptr, &raw mut dec_url_handle);
    assert_eq!(rc_from_url, 0);
    assert_ne!(dec_url_handle, 0);

    // ToText
    let mut text_out_ptr: *mut c_char = std::ptr::null_mut();
    let rc_to_text = bn_rt_crypto_bytes_to_text(handle, &raw mut text_out_ptr);
    assert_eq!(rc_to_text, 0);
    assert!(!text_out_ptr.is_null());
    let text_str = unsafe { CStr::from_ptr(text_out_ptr) }
        .to_str()
        .expect("valid utf-8");
    assert_eq!(text_str, "Hello ABI Encoding!");
    bn_rt_str_release(text_out_ptr);

    // Concat
    let mut concat_handle = 0i64;
    let rc_concat = bn_rt_crypto_bytes_concat(handle, handle, &raw mut concat_handle);
    assert_eq!(rc_concat, 0);
    assert_eq!(
        bn_rt_crypto_bytes_length(concat_handle.cast_unsigned()),
        i64::try_from(raw.len() * 2).unwrap()
    );

    // ToPem and FromPem
    let label = BnString::new(b"TEST MESSAGE");
    let pem_ptr = bn_rt_crypto_to_pem(label.as_ptr(), handle);
    assert!(!pem_ptr.is_null());
    let mut pem_dec_handle = 0i64;
    let rc_from_pem = bn_rt_crypto_from_pem(pem_ptr, &raw mut pem_dec_handle);
    assert_eq!(rc_from_pem, 0);
    assert_eq!(
        bn_rt_crypto_bytes_length(pem_dec_handle.cast_unsigned()),
        i64::try_from(raw.len()).unwrap()
    );

    // Checksums C ABI
    let crc = bn_rt_crypto_crc32(handle);
    assert_ne!(crc, 0);

    let crc_c = bn_rt_crypto_crc32c(handle);
    assert_ne!(crc_c, 0);

    let adler = bn_rt_crypto_adler32(handle);
    assert_ne!(adler, 0);

    let xx64 = bn_rt_crypto_xxhash64(handle);
    assert_ne!(xx64, 0);

    // VerifyFileChecksum C ABI
    let path = BnString::new(b"test_c_abi_checksum.tmp");
    fs::write("test_c_abi_checksum.tmp", b"Hello ABI Encoding!").expect("write tmp file");
    let spec = BnString::new(format!("crc32:{crc:08x}").as_bytes());
    let v_rc = bn_rt_crypto_verify_file_checksum(path.as_ptr(), spec.as_ptr());
    assert_eq!(v_rc, 1);
    let _ = fs::remove_file("test_c_abi_checksum.tmp");

    bn_rt_str_release(b64_ptr);
    bn_rt_str_release(b64url_ptr);
    bn_rt_str_release(pem_ptr);
    let _ = bn_rt_crypto_bytes_release(handle);
    let _ = bn_rt_crypto_bytes_release(dec_handle.cast_unsigned());
    let _ = bn_rt_crypto_bytes_release(dec_url_handle.cast_unsigned());
    let _ = bn_rt_crypto_bytes_release(concat_handle.cast_unsigned());
    let _ = bn_rt_crypto_bytes_release(pem_dec_handle.cast_unsigned());
}
