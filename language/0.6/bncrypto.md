# BNCrypto standard library

## Status

External standard-library module. Source of truth:
`modules/bn/BNCrypto.bn`. Cryptographic primitives are implemented through
`bn_rt::crypto` over the shared C ABI, ensuring that the interpreter (`bni`,
through the `bn_lib_crypto` provider) and compiled native executables (`bnc`,
through the LLVM backend) execute the identical implementation.

This specification defines the cryptographic architecture: the Two-Layer API,
CSPRNG generation, memory zeroization, typed key handles, authenticated envelope
encryption, cryptographic hashes, MACs, password hashing (PHC format), digital
signatures, post-quantum primitives, and error contracts.

## Access

```basic
IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    LET key AS Crypto.SecretKey OR Error = Crypto.NewSecretKey()
    IF key IS Crypto.SecretKey THEN
        LET envelope AS Crypto.Bytes OR Error = Crypto.Encrypt(key, Crypto.FromText("secret"))
    END IF
END FUNCTION
```

`BNCrypto` is an imported standard library module, never a `HOST` capability.

---

## Two-Layer API Design

Basic Next 0.6 has no method overloading, optional parameters, or named parameters.
To provide simple, safe defaults for typical use cases while enabling fine-grained
tuning for advanced applications without duplicating library abstractions, the API
follows the Two-Layer pattern:

* **Layer A (Need):** Direct functions with safe, hardened defaults (e.g., `NewSecretKey()`, `NewKeyPair()`, `Encrypt(key, data)`, `HashPassword(password)`).
* **Layer B (Options):** Companion functions taking an options `STRUCT` with initializers, named with the `With` suffix (e.g., `NewSecretKeyWith(opts)`, `EncryptWith(key, data, opts)`, `HashPasswordWith(password, opts)`). The user instantiates the options `STRUCT` and modifies only the parameters they need to tune.

Entities use `STATIC` factories. Operations that fail return `T OR Error`.

---

## Algorithm Registry

Cryptographic algorithms are selected using typed integer constants exposed by the static class `Alg`:

```basic
EXPORT CLASS Alg
    // Hashes (BDFL decision, 2026-11-01)
    PUBLIC STATIC SHA256 AS INTEGER = 1
    PUBLIC STATIC SHA384 AS INTEGER = 2
    PUBLIC STATIC SHA512 AS INTEGER = 3
    PUBLIC STATIC SHA512_256 AS INTEGER = 4
    PUBLIC STATIC SHA3_256 AS INTEGER = 5
    PUBLIC STATIC SHA3_512 AS INTEGER = 6
    PUBLIC STATIC BLAKE2B AS INTEGER = 7
    PUBLIC STATIC BLAKE3 AS INTEGER = 8
    PUBLIC STATIC MD5 AS INTEGER = 9
    PUBLIC STATIC SHA1 AS INTEGER = 10

    // MAC
    PUBLIC STATIC HMAC_SHA256 AS INTEGER = 11
    PUBLIC STATIC HMAC_SHA384 AS INTEGER = 12
    PUBLIC STATIC HMAC_SHA512 AS INTEGER = 13
    PUBLIC STATIC BLAKE3_KEYED AS INTEGER = 14

    // AEAD
    PUBLIC STATIC AES_256_GCM AS INTEGER = 20
    PUBLIC STATIC CHACHA20_POLY1305 AS INTEGER = 21
    PUBLIC STATIC XCHACHA20_POLY1305 AS INTEGER = 22

    // Asymmetric & Post-Quantum (BDFL decision, 2026-11-04)
    PUBLIC STATIC ED25519 AS INT32 = 30
    PUBLIC STATIC X25519 AS INT32 = 31
    PUBLIC STATIC ECDSA_P256 AS INT32 = 32
    PUBLIC STATIC ML_KEM_768 AS INTEGER = 40
    PUBLIC STATIC ML_DSA_65 AS INTEGER = 41
END CLASS

EXPORT CLASS PemFormat
    PUBLIC STATIC PKCS8 AS INT32 = 1
END CLASS
```

`INTEGER` is `INT32`. Key-generation selectors and the PEM format spell `INT32` so the width is part of the signature (BDFL decision, 2026-11-04).

---

## Types and Memory Safety

| Type | Nature | Lifecycle and Safety |
| --- | --- | --- |
| `Crypto.Bytes` | Opaque buffer | Owned byte buffer. Does not implicitly convert to `STRING`. Wiped (`zeroize`) upon last `RELEASE`. Supports `.Length()`, `.ToHex()`, `.ToText()`. |
| `Crypto.SecretKey` | Opaque symmetric key | Zeroized on drop or `RELEASE`. Memory locked (`mlock`) where platform permits. Not printable; `ToHex()` is forbidden. Exports only via explicit `ExportSecret`. |
| `Crypto.PrivateKey` | Opaque asymmetric private key | Zeroized on drop or `RELEASE`. Memory locked (`mlock`) where platform permits. Exports only encrypted via `.ExportPem(format, password)` or explicit `.ExportPemUnencrypted(format)`. After `.Zeroize()`, `.PublicKey()`, `.Id()` and `.Algorithm()` still answer: they hold no secret (BDFL decision, 2026-11-05). |
| `Crypto.PublicKey` | Opaque asymmetric public key | Public key representation. Supports `.ExportPem()`, `.Fingerprint(hashAlg)`. |
| `Crypto.KeyPair` | `STRUCT` | Contains `PublicKey AS PublicKey` and `PrivateKey AS PrivateKey`. |
| `Crypto.Secret` | Sensitive text | Prints as `"***"`. Cannot be concatenated. Converts explicitly to `Bytes`. |

### Key Identifiers (KeyId)

Every key carries a KeyId (`key.Id() AS STRING`):
* For asymmetric keys (`PublicKey`, `PrivateKey`): SHA-256 digest of the canonical SPKI (SubjectPublicKeyInfo) DER encoding, represented as a 64-character lowercase hexadecimal string.
* For symmetric keys (`SecretKey`): SHA-256 digest of `("BNC-KEYID\0" || raw_bytes)`, represented as a 64-character lowercase hexadecimal string.

---

## Randomness and CSPRNG

All cryptographic randomness derives directly from the operating system CSPRNG (`getrandom` / `OsRng`).

| Function | Signature | Behaviour |
| --- | --- | --- |
| `RandomBytes` | `RandomBytes(length AS INTEGER) AS Bytes OR Error` | Returns `length` secure random bytes (0 to 1 MiB). Rejects length outside range. |
| `RandomInt` | `RandomInt(min, max AS INTEGER) AS INTEGER OR Error` | Unbiased random integer in `[min, max]` via rejection sampling. Rejects `min > max`. |
| `RandomToken` | `RandomToken(nBytes AS INTEGER) AS STRING OR Error` | URL-safe base64 string from `nBytes` random bytes without padding. Rejects `nBytes <= 0` or length above 1 MiB. |
| `RandomUuidV4` | `RandomUuidV4() AS STRING` | Canonical RFC 4122 UUID version 4 string. |
| `UuidV7` | `UuidV7() AS STRING` | Time-ordered RFC 9562 UUID version 7 string. |
| `RandomPassword` | `RandomPassword(opts AS PasswordGenOptions) AS STRING OR Error` | Secure password generated with configurable character classes and length (default 20). |

**Prohibition:** The CSPRNG never accepts a user seed in production. Cryptographic nonces and keys must never be derived from pseudo-random generators (`HOST.Random`); they must be drawn directly from the operating system CSPRNG via `Crypto.RandomBytes`.

---

## Constant-Time Operations and Zeroization

* `ConstantTimeEquals(a, b AS Bytes) AS BOOLEAN`: Compares two byte buffers in constant time without short-circuiting, mitigating side-channel timing attacks.
* `Zeroize(b AS Bytes) AS VOID`: Explicitly overwrites buffer memory with zeros. Temporary allocations in cryptographic primitives are zeroized prior to return.

---

## Authenticated Symmetric Encryption (AEAD)

### Sealed Envelopes (Layer A)

High-level encryption produces a versioned, self-contained envelope with an internal CSPRNG nonce:

| Function | Signature |
| --- | --- |
| `Encrypt` | `Encrypt(key AS SecretKey, plaintext AS Bytes) AS Bytes OR Error` |
| `Decrypt` | `Decrypt(key AS SecretKey, sealed AS Bytes) AS Bytes OR Error` |
| `EncryptText` | `EncryptText(key AS SecretKey, plaintext AS STRING) AS STRING OR Error` |
| `DecryptText` | `DecryptText(key AS SecretKey, sealed AS STRING) AS STRING OR Error` |

Default algorithm: **XChaCha20-Poly1305** (24-byte random nonce, safe against nonce reuse).

### Envelope Format (`BNC1`)

```
+---------------+---------+--------+------------------+----------+------------+----------+
| Magic "BNC1"  | Version | Alg ID | KeyId (optional) |  Nonce   | Ciphertext |   Tag    |
|   (4 bytes)   | (1 byte)|(1 byte)|   (32 bytes)     | (N bytes)| (L bytes)  |(16 bytes)|
+---------------+---------+--------+------------------+----------+------------+----------+
```
The header bytes preceding the ciphertext are included in the authenticated additional data (AAD).

### Options & Streaming (Layer B)

```basic
EXPORT STRUCT AeadOptions
    alg AS INTEGER = 4           // Crypto.Alg.XCHACHA20_POLY1305
    aad AS Bytes                 // Additional authenticated data (default empty)
    includeKeyId AS BOOLEAN = 0  // Whether to embed 32-byte KeyId in envelope
END STRUCT
```

* `EncryptWith(key AS SecretKey, plaintext AS Bytes, opts AS AeadOptions) AS Bytes OR Error`
* `DecryptWith(key AS SecretKey, sealed AS Bytes, opts AS AeadOptions) AS Bytes OR Error`
* `EncryptFile(key AS SecretKey, inPath, outPath AS STRING) AS VOID OR Error`
* `DecryptFile(key AS SecretKey, inPath, outPath AS STRING) AS VOID OR Error`
* `EncryptFileWithPassword(inPath, outPath, password AS STRING) AS VOID OR Error`
* `DecryptFileWithPassword(inPath, outPath, password AS STRING) AS VOID OR Error`

File encryption streams in 64 KiB authenticated chunks. New target files use atomic creation (`FS.OpenNew`). Decryption failure deletes partial output and fails closed.

### Raw AEAD (Legacy / Expert)

`SealAesGcm`, `OpenAesGcm`, `SealChaCha20`, and `OpenChaCha20` operate over `Key` and accept explicit caller-supplied nonces (12 bytes). They are preserved for wire compatibility and interop, but are documented as hazardous:

* **Nonce reuse catastrophe:** Reusing a nonce with the same key destroys both confidentiality and authenticity under AES-GCM and ChaCha20-Poly1305.
* **Key rotation warning:** When generating random 96-bit (12-byte) nonces with AES-GCM, the birthday paradox mandates rotating the key well before 2^32 encryptions (NIST SP 800-38D recommends rotation at or before 2^32 invocations).
* **Safe alternative:** Prefer the higher-level Two-Layer API (`Encrypt`/`Decrypt` or `EncryptFile`/`DecryptFile`), which derives safe internal nonces (e.g. 24-byte nonces for XChaCha20-Poly1305 or per-segment nonces) automatically.
* **Forbidden unauthenticated modes:** Electronic Codebook (ECB), unauthenticated Cipher Block Chaining (CBC), unauthenticated Counter (CTR), RC4, and DES/3DES are strictly rejected. Unauthenticated modes fail closed with `INVALID_ARGUMENT`. Legacy CBC+HMAC decryption is restricted to explicit legacy policy (see Part 5).

---

## Cryptographic Hashes

| Function | Signature |
| --- | --- |
| `Hash` | `Hash(data AS Bytes) AS Bytes` (default SHA-256) |
| `HashText` | `HashText(text AS STRING) AS STRING` (lowercase hex) |
| `HashFile` | `HashFile(path AS STRING) AS Bytes OR Error` (streaming from disk) |
| `HashWith` | `HashWith(data AS Bytes, opts AS HashOptions) AS Bytes OR Error` |

Supported in `HashOptions.alg`: SHA-256, SHA-384, SHA-512, SHA-512/256, SHA3-256, SHA3-512, BLAKE2b, BLAKE3.
Streaming hash updates are provided via `Hasher.New(alg)` and `Hasher.NewWith(opts AS HashOptions)`, which applies the options' algorithm and policy. `Alg.MD5` and `Alg.SHA1` are reserved: every request for them is `LEGACY_DISABLED`, with or without a policy (BDFL decision, 2026-11-05).

---

## Message Authentication (MAC)

| Function | Signature |
| --- | --- |
| `Mac` | `Mac(key AS SecretKey, data AS Bytes) AS Bytes OR Error` — default HMAC-SHA-256 |
| `VerifyMac` | `VerifyMac(key AS SecretKey, data, tag AS Bytes) AS BOOLEAN` |
| `MacWith` | `MacWith(key AS SecretKey, data AS Bytes, opts AS MacOptions) AS Bytes OR Error` |

`VerifyMac` operates in constant time. Streaming MAC computation is supported via `MacStream.New(key, opts)`.

---

## Passwords and Key Derivation

| Function | Signature | Description |
| --- | --- | --- |
| `HashPassword` | `HashPassword(password AS STRING) AS STRING` | Produces an Argon2id PHC string with OWASP recommended floor (m=19456 KiB, t=2, p=1, 16-byte random salt) (BDFL decision, 2026-11-01). |
| `VerifyPassword` | `VerifyPassword(password AS STRING, phc AS STRING) AS BOOLEAN` | Constant-time verification against an Argon2id PHC string. |
| `NeedsRehash` | `NeedsRehash(phc AS STRING, opts AS PasswordOptions) AS BOOLEAN` | Checks if a PHC string is below target cost parameters. |
| `DeriveKey` | `DeriveKey(password AS STRING, salt AS Bytes) AS SecretKey OR Error` | Password-based key derivation using Argon2id. |
| `Hkdf` | `Hkdf(ikm AS SecretKey, salt, info AS Bytes, length AS INTEGER) AS SecretKey OR Error` | RFC 5869 extract-and-expand key derivation returning a typed `SecretKey`. |
| `DeriveSubkey` | `DeriveSubkey(master AS SecretKey, context AS STRING) AS SecretKey OR Error` | Domain-separated subkey derivation. |

---

## Encoding Codecs and Utilities

| Function | Signature | Description |
| --- | --- | --- |
| `ToText` | `ToText(data AS Bytes) AS STRING OR Error` | Validates strict UTF-8 and returns `STRING`. Fails with `INVALID_ARGUMENT` if malformed. Also available as `.ToText()` method on `Bytes`. |
| `ToBase64` | `ToBase64(data AS Bytes) AS STRING` | RFC 4648 §4 standard Base64 encoding. Also available as `.ToBase64()` on `Bytes`. |
| `FromBase64` | `FromBase64(text AS STRING) AS Bytes OR Error` | Decodes standard Base64. |
| `ToBase64Url` | `ToBase64Url(data AS Bytes) AS STRING` | RFC 4648 §5 URL- and filename-safe Base64 without padding. Also available as `.ToBase64Url()` on `Bytes`. |
| `FromBase64Url` | `FromBase64Url(text AS STRING) AS Bytes OR Error` | Decodes URL-safe Base64. |
| `Concat` | `Concat(a AS Bytes, b AS Bytes) AS Bytes` | Concatenates two byte buffers into an owned byte buffer. |
| `ToPem` | `ToPem(label AS STRING, der AS Bytes) AS STRING` | Encodes binary DER payload into PEM with `-----BEGIN <LABEL>-----` and `-----END <LABEL>-----` boundaries. |
| `FromPem` | `FromPem(pem AS STRING) AS Bytes OR Error` | Parses PEM and extracts the binary DER bytes. Malformed PEM is `PARSE_ERROR`. |

---

## Checksums

Non-cryptographic integrity checks are grouped under the `Checksum` class:

| Function | Signature | Description |
| --- | --- | --- |
| `Crc32` | `Checksum.Crc32(data AS Bytes) AS INT64` | Computes 32-bit CRC (IEEE 802.3 standard polynomial `0xEDB88320`). |
| `Crc32c` | `Checksum.Crc32c(data AS Bytes) AS INT64` | Computes 32-bit CRC-32C (Castagnoli polynomial `0x82F63B78`). |
| `Adler32` | `Checksum.Adler32(data AS Bytes) AS INT64` | Computes 32-bit Adler checksum (RFC 1950). |
| `XxHash64` | `Checksum.XxHash64(data AS Bytes) AS INT64` | Computes fast 64-bit non-cryptographic xxHash with default seed 0. |
| `VerifyFileChecksum` | `Checksum.VerifyFileChecksum(path AS STRING, expected AS STRING) AS BOOLEAN` | Streams file content and verifies against an algorithm-tagged specifier (e.g. `"sha256:<hex>"`, `"crc32:<hex>"`). |

---

## Asymmetric Key Pairs and Signatures

### Key Generation

| Function | Signature | Description |
| --- | --- | --- |
| `NewKeyPair` | `NewKeyPair() AS KeyPair OR Error` | Generates an Ed25519 key pair using system CSPRNG. The key's algorithm is `Alg.ED25519` (30). |
| `NewKeyPairFor` | `NewKeyPairFor(alg AS INT32) AS KeyPair OR Error` | Supports `Alg.ED25519` (30), `Alg.X25519` (31), `Alg.ECDSA_P256` (32). |
| `NewKeyPairWith` | `NewKeyPairWith(opts AS KeyPairOptions) AS KeyPair OR Error` | Configured generation. `KeyPairOptions` is below. No seed. |

`KeyPairOptions` has two fields (BDFL decisions, 2026-11-04 and 2026-11-05):

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `alg` | `INT32` | `Alg.ED25519` (30) | Algorithm to generate. A seed and a label are not fields of this version. |
| `policy` | `Policy` | default `Policy` | See "Per-Call Cryptographic Policy". |

### Signatures

| Function | Signature |
| --- | --- |
| `Sign` | `Sign(key AS PrivateKey, message AS Bytes) AS Bytes OR Error` |
| `Verify` | `Verify(key AS PublicKey, message, signature AS Bytes) AS BOOLEAN` |
| `SignFile` | `SignFile(key AS PrivateKey, path AS STRING) AS Bytes OR Error` |
| `VerifyFile` | `VerifyFile(key AS PublicKey, path AS STRING, signature AS Bytes) AS BOOLEAN` |

Deterministic signatures (RFC 8032 for Ed25519, RFC 6979 for ECDSA P-256). `Verify` returns `FALSE` for invalid signatures or mismatched keys. Ed25519 verification is strict: a signature whose public key or `R` component has small order is `FALSE`.

### Key Import and Export

* `ExportPem(key AS PrivateKey, format AS INT32, password AS STRING) AS STRING OR Error`: Encrypts private keys using PKCS#8 with PBES2 (scrypt N = 2^17, r = 8, p = 1; AES-256-CBC). `format` is `PemFormat.PKCS8` (1). Any other `INT32` is `INVALID_ARGUMENT` (BDFL decision, 2026-11-04).
* `ExportPemUnencrypted(key AS PrivateKey, format AS INT32) AS STRING OR Error`: Explicit unencrypted PKCS#8 export. The same `format` values.
* `ExportPem(key AS PublicKey) AS STRING OR Error`: Exports SPKI PEM. No format parameter.
* `LoadPrivateKey(pem AS STRING, password AS STRING OR NULL) AS PrivateKey OR Error`: Unambiguous header detection; fails closed (`PARSE_ERROR`) on ambiguous inputs. The key-derivation cost of an encrypted key is checked before any derivation runs: scrypt with N above 2^17, r above 8 or p above 1, PBKDF2 above 2,000,000 iterations, or another KDF is `PARSE_ERROR`.
* `LoadPublicKey(pem AS STRING) AS PublicKey OR Error`: Parses SPKI PEM. An Ed25519 key that is not a point of large order (a weak key) is `PARSE_ERROR`.

---

## Post-Quantum Cryptography (ML-KEM and ML-DSA)

* ML-KEM-768 (FIPS 203) key encapsulation: `MlKemEncapsulate`, `MlKemDecapsulate`.
* ML-DSA-65 (FIPS 204) digital signatures: `MlDsaSign`, `MlDsaVerify`.
* Decapsulation follows **implicit rejection**: a well-sized invalid ciphertext produces an independent pseudo-random shared secret rather than `AUTHENTICATION_FAILED`.

---

## Errors

Error codes returned by BNCrypto operations:

| Constant | Value | Condition |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | Parameter out of accepted range; zeroized key; length constraint violation |
| `AUTHENTICATION_FAILED` | 2 | AEAD tag verification failure, tampered envelope or ciphertext |
| `UNAVAILABLE` | 3 | Primitive or hardware feature not supported in current environment |
| `UNSUPPORTED_ALGORITHM` | 4 | Requested algorithm ID not recognized or not enabled |
| `LEGACY_DISABLED` | 5 | Insecure or legacy algorithm requested without explicit policy opt-in |
| `PARSE_ERROR` | 6 | Malformed PEM, DER, PHC, or serialized cryptographic structure |
| `WRONG_KEY_TYPE` | 7 | Key type mismatch (e.g., signing key supplied to encryption operation) |
| `IO_ERROR` | 8 | File I/O failure during streaming operations |
| `WRONG_PASSWORD` | 9 | Decryption password failed authentication |

---

## Per-Call Cryptographic Policy

A policy is a field of the options record of the call that needs it (BDFL decision, 2026-11-05). It is a value of that call only: there is no process-wide switch and no environment variable, and a policy never reaches another call.

```basic
EXPORT STRUCT Policy
    allowLegacy AS BOOLEAN = FALSE
    fipsOnly AS BOOLEAN = FALSE
END STRUCT
```

`HashOptions`, `MacOptions` and `KeyPairOptions` carry `policy AS Policy` as their last field. A call without options (`Hash`, `Mac`, `NewKeyPair`, …) uses the default policy.

| Field | Effect in this version |
| --- | --- |
| `allowLegacy` | Reserved. No legacy algorithm is available in this version: `Alg.MD5` and `Alg.SHA1` are `LEGACY_DISABLED` whatever the policy (BDFL decision, 2026-11-05). |
| `fipsOnly` | The call refuses, with `UNSUPPORTED_ALGORITHM`, an algorithm its options select that is not FIPS-approved: `Alg.BLAKE2B` and `Alg.BLAKE3` in `HashOptions`, `Alg.BLAKE3_KEYED` in `MacOptions`, `Alg.X25519` in `KeyPairOptions`. |

`fipsOnly` is an algorithm filter, not FIPS 140-3 validation: the runtime is not a validated cryptographic module. Functions whose algorithm is fixed and is not FIPS-approved (`HashPassword` and `DeriveKey` with Argon2id, envelope encryption with XChaCha20-Poly1305, `ExportPem` with scrypt) take no options record, so no policy reaches them; a program that needs only approved algorithms does not call them.
