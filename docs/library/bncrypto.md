# BNCrypto standard library

## Status

External standard-library module (bucket 0.6.1b). Source of truth:
`modules/bn/BNCrypto.bn`. The digests are **not** implemented in Basic Next: they
come from `bn_rt::crypto` over the C ABI, so the interpreter (`bni`, through the
`bn_lib_crypto` provider) and compiled binaries (`bnc`, through the LLVM backend)
execute one implementation and cannot drift.

This document covers what has landed: the digests and the opaque `Bytes` buffer.
Authenticated encryption, message authentication, key derivation, signatures, and
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

## Target support

| Member | `bni` | `bnc` |
| --- | --- | --- |
| `SHA256`, `SHA512` | yes | yes |
| `FromText`, `Length`, `ToHex`, `RELEASE` | yes | yes |
| `FromHex` | yes | **no** — `TARGET_UNSUPPORTED_OP` |

The compiled path routes digests through `bn_rt_crypto_sha256` /
`bn_rt_crypto_sha512`, which return an owned NUL-terminated UTF-8 string under
the same ownership convention as `bn_rt_str_to_lower`. A `Bytes` handle travels
as a pointer carrying the `bn_rt` table index, as `BNLog` resources do.

`FromHex` is interpret-only because it returns `Bytes OR Error`, and the backend
does not yet emit the narrowing for that aggregate. This is a **support** gap,
not a language one: compiling a program that calls it fails with
`TARGET_UNSUPPORTED_OP` rather than miscompiling, and a test pins that
behaviour so the boundary cannot rot into a silent wrong answer.

## Evidence

| Claim | Check |
| --- | --- |
| Digests match FIPS 180-4 | `cargo test -p bn_rt crypto` |
| Hex rejects odd-length input | `cargo test -p bn_rt crypto` |
| Handles are distinct and release once | `cargo test -p bn_rt crypto_abi` |
| Interpreter serves `IMPORT BNCrypto` | `cargo test -p bni --test cli bncrypto` |
| Compiled digests equal interpreted digests | `cargo test -p bnc --test cli compiled_bncrypto_digests` |
| Compiled `Bytes` equals interpreted `Bytes` | `cargo test -p bnc --test cli compiled_bncrypto_bytes` |
| `FromHex` fails as *support*, not as a language error | `cargo test -p bnc --test cli compiled_bncrypto_from_hex` |

## Not here

AEAD (`AES-256-GCM`, `ChaCha20-Poly1305`), `HMAC-SHA-256`, `Argon2id`, `Ed25519`,
`ECDSA P-256`, `ML-KEM-768`, and `ML-DSA-65`. Those follow in later activities of
bucket 0.6.1b or a successor bucket; none of them is importable today. They all
build on `Crypto.Bytes`, which is why the handle landed before any cipher.
