# BNCrypto standard library

## Status

External standard-library module (bucket 0.6.1b). Source of truth:
`modules/bn/BNCrypto.bn`. The digests are **not** implemented in Basic Next: they
come from `bn_rt::crypto` over the C ABI, so the interpreter (`bni`, through the
`bn_lib_crypto` provider) and compiled binaries (`bnc`, through the LLVM backend)
execute one implementation and cannot drift.

This document covers the digest surface only. Authenticated encryption, message
authentication, key derivation, signatures, and the post-quantum primitives named
in the bucket are **not** part of this cut and are not yet importable.

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

## Target support

Both backends support `BNCrypto` digests. The compiled path routes through
`bn_rt_crypto_sha256` / `bn_rt_crypto_sha512`, which return an owned
NUL-terminated UTF-8 string under the same ownership convention as
`bn_rt_str_to_lower`. A call whose argument is not a `STRING` at the IR level
fails the target support check (`TARGET_UNSUPPORTED_OP`), which is a support
diagnostic, not a language error.

## Evidence

| Claim | Check |
| --- | --- |
| Digests match FIPS 180-4 | `cargo test -p bn_rt crypto` |
| Interpreter serves `IMPORT BNCrypto` | `cargo test -p bni --test cli bncrypto` |
| Compiled output equals interpreted output | `cargo test -p bnc --test cli compiled_bncrypto` |

## Not here

`Crypto.Bytes` (the opaque byte handle the remaining families need), AEAD
(`AES-256-GCM`, `ChaCha20-Poly1305`), `HMAC-SHA-256`, `Argon2id`, `Ed25519`,
`ECDSA P-256`, `ML-KEM-768`, and `ML-DSA-65`. Those follow in later activities of
bucket 0.6.1b or a successor bucket; none of them is importable today.
