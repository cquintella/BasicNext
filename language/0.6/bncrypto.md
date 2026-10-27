# BNCrypto standard library

## Status

External standard-library module (bucket 0.6.1b). Source of truth:
`modules/bn/BNCrypto.bn`. The digests are **not** implemented in Basic Next: they
come from `bn_rt::crypto` over the C ABI, so the interpreter (`bni`, through the
`bn_lib_crypto` provider) and compiled binaries (`bnc`, through the LLVM backend)
execute one implementation and cannot drift.

This document covers the whole module: digests, the opaque `Bytes` buffer,
authenticated encryption, message authentication, password hashing, signatures,
and the post-quantum primitives. Every member runs on both backends.

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
| `Crypto.Bytes` | Opaque byte buffer. No implicit conversion to `STRING`, so byte data cannot be printed, concatenated or logged by accident. Owned: `RELEASE` frees it, and releasing twice is reported. |
| `Crypto.Key` | Opaque cryptographic key or private seed. Never converts to `STRING` or exposes raw hex. Guarantees memory zeroization (`ZeroizeOnDrop`) upon scope exit or `RELEASE`. Supports explicit `.Zeroize()`. |

| Function | Signature | Result |
| --- | --- | --- |
| `FromText` | `FromText(text AS STRING) AS Bytes` | The UTF-8 bytes of `text` |
| `FromHex` | `FromHex(text AS STRING) AS Bytes OR Error` | Decoded bytes, or `Error` when the string is not even-length hex |
| `RandomBytes` | `RandomBytes(length AS INTEGER) AS Bytes OR Error` | Cryptographically secure random bytes from operating-system entropy |
| `GenerateNonce` | `GenerateNonce(cipherAlgorithm AS INTEGER) AS Bytes OR Error` | 12-byte CSPRNG nonce suitable for AEAD ciphers |
| `GenerateKey` | `GenerateKey(algorithm AS INTEGER) AS Key OR Error` | Random key generated via CSPRNG with algorithm-specific length |
| `ImportKey` | `ImportKey(material AS Bytes, algorithm AS INTEGER) AS Key OR Error` | Validates length for `algorithm` and encapsulates into a zeroizing `Key` |
| `Length` | `<bytes>.Length() AS INTEGER` | Byte count |
| `ToHex` | `<bytes>.ToHex() AS STRING` | Lowercase hex (for `Bytes` only; `Key` does not support `ToHex`) |

`FromHex` rejects odd-length input instead of truncating: an odd number of hex
characters is not a byte sequence.

### Algorithm Constants

| Constant | Value | Algorithm / Key Length |
| --- | ---: | --- |
| `ALG_AES_GCM_256` | 1 | AES-256-GCM (32-byte key, 12-byte nonce) |
| `ALG_CHACHA20` | 2 | ChaCha20-Poly1305 (32-byte key, 12-byte nonce) |
| `ALG_HMAC_SHA256` | 3 | HMAC-SHA-256 (arbitrary key, minimum 32 recommended) |
| `ALG_ED25519` | 4 | Ed25519 signing seed (32 bytes) |
| `ALG_ECDSA_P256` | 5 | ECDSA P-256 private scalar (32 bytes) |
| `ALG_ML_KEM_768` | 6 | ML-KEM-768 seed (64 bytes) |
| `ALG_ML_DSA_65` | 7 | ML-DSA-65 signing seed (32 bytes) |

## Authenticated encryption

| Function | Signature |
| --- | --- |
| `SealAesGcm` | `SealAesGcm(key AS Key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error` |
| `OpenAesGcm` | `OpenAesGcm(key AS Key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error` |
| `SealChaCha20` | `SealChaCha20(key AS Key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error` |
| `OpenChaCha20` | `OpenChaCha20(key AS Key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error` |

AES-256-GCM follows **NIST SP 800-38D**; ChaCha20-Poly1305 follows **RFC 7539**.
Both take a **32-byte `Key`** and a **12-byte nonce**, and both append a **16-byte
authentication tag** to the ciphertext. The additional authenticated data may be
empty, but it is authenticated, not encrypted.

`Open` **fails closed**. A tampered ciphertext, a changed AAD, a wrong key or a
truncated tag all yield `Error` and no plaintext whatsoever — never a partial or
unauthenticated result. `Seal` yields `Error` when the key or nonce length is
wrong rather than padding or truncating it.

A nonce must never be reused with the same key. Derive nonces using `Crypto.GenerateNonce(cipherAlgorithm)` or `Crypto.RandomBytes(12)` from the system CSPRNG, or maintain a strictly monotonic counter. Nonces must never be derived from pseudo-random number generators (`HOST.Random`).

## Message authentication and password hashing

| Function | Signature |
| --- | --- |
| `HmacSha256` | `HmacSha256(key AS Key, data AS Bytes) AS Bytes` |
| `VerifyHmacSha256` | `VerifyHmacSha256(key AS Key, data, tag AS Bytes) AS BOOLEAN` |
| `Argon2id` | `Argon2id(password, salt AS Bytes, memoryKiB, iterations, parallelism AS INTEGER) AS Bytes OR Error` |

HMAC-SHA-256 follows RFC 2104. **`VerifyHmacSha256` compares in constant time**: it
never short-circuits on the first differing byte, which would leak the tag to a
timing attack.

Argon2id follows RFC 9106 and returns a 32-byte tag. Cost parameters outside the
algorithm's accepted range are **rejected**, never clamped to something weaker.

## Signatures

| Function | Signature |
| --- | --- |
| `Ed25519PublicKey` | `Ed25519PublicKey(key AS Key) AS Bytes OR Error` |
| `Ed25519Sign` | `Ed25519Sign(key AS Key, message AS Bytes) AS Bytes OR Error` |
| `Ed25519Verify` | `Ed25519Verify(publicKey, message, signature AS Bytes) AS BOOLEAN` |
| `EcdsaP256PublicKey` | `EcdsaP256PublicKey(key AS Key) AS Bytes OR Error` |
| `EcdsaP256Sign` | `EcdsaP256Sign(key AS Key, message AS Bytes) AS Bytes OR Error` |
| `EcdsaP256Verify` | `EcdsaP256Verify(publicKey, message, signature AS Bytes) AS BOOLEAN` |

Ed25519 keys are a 32-byte seed (RFC 8032). ECDSA P-256 keys are a 32-byte scalar;
the public key is SEC1 uncompressed (65 bytes) and signatures are raw `r || s`
(64 bytes) over SHA-256, with the nonce derived per RFC 6979.

Both schemes sign **deterministically**: the same key and message always produce
the same signature. `Verify` returns `FALSE` for a malformed key, a tampered
message or a signature from another key — never an error resembling success.

## Post-quantum

| Function | Signature |
| --- | --- |
| `MlKemKeypair` | `MlKemKeypair(seed AS Key) AS Bytes OR Error` — `publicKey \|\| privateKey` |
| `MlKemEncapsulate` | `MlKemEncapsulate(publicKey AS Bytes) AS Bytes OR Error` — `ciphertext \|\| sharedSecret` |
| `MlKemDecapsulate` | `MlKemDecapsulate(key AS Key, ciphertext AS Bytes) AS Bytes OR Error` |
| `MlDsaKeypair` | `MlDsaKeypair(seed AS Key) AS Bytes OR Error` — `verifyingKey \|\| seed` |
| `MlDsaSign` | `MlDsaSign(key AS Key, message AS Bytes) AS Bytes OR Error` |
| `MlDsaVerify` | `MlDsaVerify(publicKey, message, signature AS Bytes) AS BOOLEAN` |
| `Slice` | `Slice(data AS Bytes, start, length AS INTEGER) AS Bytes OR Error` |

ML-KEM-768 follows **FIPS 203** and ML-DSA-65 follows **FIPS 204**. BN has no
tuple, so the members that produce two values concatenate them and `Slice` splits
them; the sizes are fixed by the standard:

| Object | Bytes |
| --- | --- |
| ML-KEM-768 public key | 1184 |
| ML-KEM-768 private key (seed) | 64 |
| ML-KEM-768 ciphertext | 1088 |
| Shared secret | 32 |
| ML-DSA-65 verifying key | 1952 |
| ML-DSA-65 seed | 32 |
| ML-DSA-65 signature | 3309 |

`Slice` rejects an out-of-range window instead of clamping it, so a bad offset can
never yield a short buffer that looks like a key.

Key generation is deterministic from its seed. **Encapsulation is not, and
deliberately offers no deterministic variant**: reusing its randomness even once
is a catastrophic failure, so the choice is not exposed to BN at all.

### A weaker verification basis, stated plainly

Every other family in this module is checked byte-for-byte against an independent
implementation. The post-quantum primitives are not, because no independent
ML-KEM / ML-DSA implementation was available to check against. What is verified
here: round-trip agreement, key-generation determinism, encapsulation
non-determinism, tamper rejection, and object sizes equal to the FIPS parameters.
The algorithms themselves rest on the upstream crates' Wycheproof suites
(`ml-kem`, `ml-dsa`, both RustCrypto). Treat this as integration-verified rather
than KAT-verified locally.

## Errors (0.6.2)

`Code` of a BNCrypto `Error` ([error.md](error.md)) is one of these `INTEGER`
constants of the module (`Crypto.AUTHENTICATION_FAILED` under
`IMPORT BNCrypto AS Crypto`):

| Constant | Value | When |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | A key, nonce, or seed of the wrong length or not valid for the algorithm; Argon2id cost parameters out of range; `FromHex` of odd-length or non-hex text; a `Slice` range outside the buffer |
| `AUTHENTICATION_FAILED` | 2 | `Open*` rejects the ciphertext (tampered data, AAD, key, nonce, or tag); `MlKemDecapsulate` rejects the ciphertext |
| `UNAVAILABLE` | 3 | The build provides no implementation of the algorithm |

`Verify*` return `FALSE` for any rejected input; they never return `Error`.

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
| HMAC matches an independent tag; verify is constant-time | `cargo test -p bn_rt crypto` |
| Argon2id reproduces the RFC 9106 test vector | `cargo test -p bn_rt crypto` |
| Ed25519 matches an independent implementation byte-for-byte | `cargo test -p bn_rt crypto` |
| P-256 accepts an OpenSSL-produced signature | `cargo test -p bn_rt crypto` |
| PQC round-trips, rejects tampering, matches FIPS sizes | `cargo test -p bn_rt crypto` |
| Signatures and PQC compile and match the interpreter | `cargo test -p bnc --test cli bncrypto` |

## Not here

OAuth / BNWeb Auth, ORM field encryption, `HOST.Ui`, full TLS / X.509, and
RSA-RS256 are outside this module by decision, not by omission; see the bucket
that scoped it.
