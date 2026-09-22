# BNCrypto standard library

## Status

External standard-library module (bucket 0.6.1b). Source of truth:
`modules/bn/BNCrypto.bn`. The digests are **not** implemented in Basic Next: they
come from `bn_rt::crypto` over the C ABI, so the interpreter (`bni`, through the
`bn_lib_crypto` provider) and compiled binaries (`bnc`, through the LLVM backend)
execute one implementation and cannot drift.

This document covers what has landed: the digests, the opaque `Bytes` buffer, and
authenticated encryption. Message authentication, key derivation, signatures, and
the post-quantum primitives named in the bucket are **not** part of this cut and
are not yet importable.

## Access

```basic
IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    PRINT Crypto.SHA256("abc")
END FUNCTION
```

Logical import name: `BNCrypto`. The alias is a convenience; the path is not used
in source. `BNCrypto` is an explicitly imported external module, never a `HOST`
capability.

## Functions

| Function | Signature | Result |
| --- | --- | --- |
| `SHA256` | `SHA256(data AS STRING) AS STRING` | 64 lowercase hex characters |
| `SHA512` | `SHA512(data AS STRING) AS STRING` | 128 lowercase hex characters |

Both hash the **UTF-8 bytes** of the argument. The digest width is fixed and does
not depend on input length. Output is always lowercase hexadecimal.

## Normative behaviour

- The digests follow **NIST FIPS 180-4**. Conformance vectors live in
  `tests/fixtures/crypto/sha-vectors.json`; their inputs are the published FIPS
  example messages and their expected values were produced by an implementation
  independent of the one under test. A mismatch is an implementation defect and
  is never resolved by editing the expectation.
- A non-`STRING` argument is a type error at the call site.
- Digests are pure: the same argument always yields the same result, with no
  host capability, no I/O, and no `HOST` permission required.

## Types

| Type | Role |
| --- | --- |
| `Crypto.Bytes` | Opaque byte buffer. No implicit conversion to `STRING`, so key material cannot be printed, concatenated or logged by accident. Owned: `RELEASE` frees it, and releasing twice is reported. |

| Function | Signature | Result |
| --- | --- | --- |
| `FromText` | `FromText(text AS STRING) AS Bytes` | The UTF-8 bytes of `text` |
| `FromHex` | `FromHex(text AS STRING) AS Bytes OR Error` | Decoded bytes, or `Error` when the string is not even-length hex |
| `Length` | `<bytes>.Length() AS INTEGER` | Byte count |
| `ToHex` | `<bytes>.ToHex() AS STRING` | Lowercase hex |

`FromHex` rejects odd-length input instead of truncating: an odd number of hex
characters is not a byte sequence.

## Authenticated encryption

| Function | Signature |
| --- | --- |
| `SealAesGcm` | `SealAesGcm(key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error` |
| `OpenAesGcm` | `OpenAesGcm(key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error` |
| `SealChaCha20` | `SealChaCha20(key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error` |
| `OpenChaCha20` | `OpenChaCha20(key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error` |

AES-256-GCM follows **NIST SP 800-38D**; ChaCha20-Poly1305 follows **RFC 7539**.
Both take a **32-byte key** and a **12-byte nonce**, and both append a **16-byte
authentication tag** to the ciphertext. The additional authenticated data may be
empty, but it is authenticated, not encrypted.

`Open` **fails closed**. A tampered ciphertext, a changed AAD, a wrong key or a
truncated tag all yield `Error` and no plaintext whatsoever — never a partial or
unauthenticated result. `Seal` yields `Error` when the key or nonce length is
wrong rather than padding or truncating it.

A nonce must never be reused with the same key. The module does not and cannot
enforce that for you; derive nonces from `HOST.Random` or a counter you control.

## Target support

Every member listed above is supported by **both** backends. Digests route
through `bn_rt_crypto_sha256` / `bn_rt_crypto_sha512`, which return an owned
NUL-terminated UTF-8 string under the same ownership convention as
`bn_rt_str_to_lower`. A `Bytes` handle travels as a pointer carrying the `bn_rt`
table index, as `BNLog` resources do; a value narrowed out of `Bytes OR Error`
keeps the `{ i1, ptr, i64 }` aggregate and the handle is extracted from it,
exactly as `FS.File` does.

A call whose argument has the wrong type fails the target support check with
`TARGET_UNSUPPORTED_OP`, which is a support diagnostic, not a language error.

## Evidence

| Claim | Check |
| --- | --- |
| Digests match FIPS 180-4 | `cargo test -p bn_rt crypto` |
| Hex rejects odd-length input | `cargo test -p bn_rt crypto` |
| Handles are distinct and release once | `cargo test -p bn_rt crypto_abi` |
| Interpreter serves `IMPORT BNCrypto` | `cargo test -p bni --test cli bncrypto` |
| Compiled digests equal interpreted digests | `cargo test -p bnc --test cli compiled_bncrypto_digests` |
| Compiled `Bytes` equals interpreted `Bytes` | `cargo test -p bnc --test cli compiled_bncrypto_bytes` |
| Compiled `FromHex` and narrowed-value methods match | `cargo test -p bnc --test cli compiled_bncrypto_bytes_full_surface` |
| AEAD matches an independent implementation | `cargo test -p bn_rt crypto` |
| AEAD fails closed on tamper, AAD change and wrong key | `cargo test -p bn_rt crypto` |
| AEAD interprets and compiles identically | `cargo test -p bnc --test cli compiled_bncrypto_aead` |

## Not here

`HMAC-SHA-256`, `Argon2id`, `Ed25519`,
`ECDSA P-256`, `ML-KEM-768`, and `ML-DSA-65`. Those follow in later activities of
bucket 0.6.1b or a successor bucket; none of them is importable today. They all
build on `Crypto.Bytes`, which is why the handle landed before any cipher.
