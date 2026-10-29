# Bucket 0.6.14 — BNCrypto Enhancements: Two-Layer API, CSPRNG, Typed Keys

**Status:** todo, created 2026-10-28. Renumbered from 0.6.12 to 0.6.14 on 2026-10-28. Starts after 0.6.12 closes (`FS.OpenNew`). It does not wait for 0.6.16. 0.6.13 is unassigned. Parts 1–5 and `EncryptFile` are this close. Certificate load and `VerifyChain` are `todo/bucket-0.6.14b-crypto-pki.md`. Issuance, HPKE, RSA above the MVP, JOSE, OTP, and SSH are `todo/bucket-0.6.17-crypto-pki-features.md`. Neither is an activity of this file. 0.6.15 does not wait for 0.6.14b or 0.6.17.
**Source:** "BNCrypto — inventário e proposta de API em duas camadas" (inventory of `modules/bn/BNCrypto.bn`, `language/0.6/bncrypto.md`, `libraries/bn_stdlib/src/crypto.rs`, `crypto_abi.rs`, `crypto_error.rs`, `libraries/bn_stdlib/Cargo.toml`, `language/0.6/host.md` (Random), `language/0.6/0.6.md` (STRUCT, STATIC factories)). It supersedes the algorithm claim in `todo/proposals/bncrypto-0.5.2c.md`, which shipped as the current module.
**Gate:** Quorra before Carlos. Reopening X.509, TLS and RSA reverses the "Not here" decision in `language/0.6/bncrypto.md`, and that call belongs to the BDFL.

Every row is an extension: spec text, red tests on `bni` and `bnc`, shared implementation in `libraries/bn_stdlib` (single `bn_rt_crypto_*` ABI), verified across Linux, macOS and Windows.

---

## Part 0: Inventory and Problems Found

### What exists today (0.6.10)
* **Backend:** RustCrypto crates inside `bn_stdlib`: `sha2`, `hmac`, `aes-gcm`, `chacha20poly1305`, `argon2`, `ed25519-dalek`, `p256` (ecdsa), `ml-kem`, `ml-dsa` and `subtle`. There is a single ABI (`bn_rt_crypto_*`) that `bni` and `bnc` share. No ring, openssl, rustls or rcgen.
* **Public API (`modules/bn/BNCrypto.bn`):**

| Area | Functions |
|---|---|
| Type | opaque `CLASS Bytes` (`Length`, `ToHex`), `FromText`, `FromHex`, `Slice` |
| Hash | `SHA256(STRING) AS STRING`, `SHA512(STRING) AS STRING` (hex) |
| AEAD | `SealAesGcm`/`OpenAesGcm`, `SealChaCha20`/`OpenChaCha20` (key 32, nonce 12, tag 16, fail-closed) |
| MAC | `HmacSha256`, `VerifyHmacSha256` (constant time via `subtle::ct_eq`) |
| Password | `Argon2id(password, salt, memoryKiB, iterations, parallelism)` → 32 bytes |
| Signature | `Ed25519PublicKey/Sign/Verify`, `EcdsaP256PublicKey/Sign/Verify` (RFC 6979, r‖s) |
| PQC | `MlKemKeypair/Encapsulate/Decapsulate` (768), `MlDsaKeypair/Sign/Verify` (65); key pairs returned concatenated |
| Errors | `INVALID_ARGUMENT=1`, `AUTHENTICATION_FAILED=2`, `UNAVAILABLE=3` |

### Problems found (real)

Reviewed against the tree on 2026-10-28. Items 1 and 3 below were true of the inventory's baseline and are only partly true of the code now.

1. **CSPRNG exists for `RandomBytes`, `GenerateNonce`, and `GenerateKey`.** They call `getrandom` in `libraries/bn_stdlib/src/crypto.rs`. `bncrypto.md` already forbids `HOST.Random` for nonces. What is still missing is the rest of Part 2 (`RandomInt`, tokens, passwords) and a test double. Do not replace the existing generator and do not add a seed.
2. **The AEAD nonce is manual.** `SealAesGcm` / `SealChaCha20` take a caller nonce. Reuse under GCM is catastrophic. `GenerateNonce` exists and does not stop a caller from reusing the result.
3. **`Key` is wiped. `Bytes` is not.** `KeyEntry` uses `zeroize`, and `bn_lib_crypto` wipes a `Key` on `Zeroize` and on the last `RELEASE`. `CryptoBytes` is dropped with `buffers.remove` and the temporary `Vec`s from `random_bytes`, `seal`, `open`, `hmac_sha256`, and the ML-KEM seed copy are ordinary allocations. A secret that lives in `Bytes` stays in memory.
4. **Private material is returned as `Bytes`.** `MlKemKeypair` and `MlDsaKeypair` append the seed (`bn_lib_crypto` `post_quantum`, `public_key.extend_from_slice`). `MlKemEncapsulate` does the same with `ciphertext || shared`. Both buffers have `ToHex`. `Key` has no `ToHex`. The new types must not repeat this: `KemResult.secret` and the result of `KeyAgreement` are `Secret` or `SecretKey`, and `Hkdf` output that is a key is `SecretKey`. The current signatures stay until a program can be moved without a silent break.
5. **Key pairs come back concatenated** and are split with `Slice` and magic offsets (1184/64…), which is fragile. `STRUCT` already exists in 0.6 and solves this.
6. **Hashes only accept STRING and return hex.** You cannot hash `Bytes` or a file, and there is no streaming.
7. **Argon2id has no defaults and no verify.** The user picks the costs and must compare hashes by hand. There is no PHC format (`$argon2id$v=19$...`).
8. **No key import/export** in PEM, DER, PKCS#8, SPKI, JWK or OpenSSH, which blocks interoperation.
9. **ML-DSA/ML-KEM verification has no local KAT** (the spec itself admits this).
10. **Too few error codes.** Missing: `UNSUPPORTED_ALGORITHM`, `LEGACY_DISABLED`, `PARSE_ERROR`, `VERIFY_FAILED` (for chains), `EXPIRED` and `IO`.

**Strengths to keep:** fail-closed behaviour; `Verify` returning FALSE; rejecting parameters instead of clamping; `Bytes` never becoming STRING; bni/bnc parity; independent test vectors.

---

## Part 1: Two-Layer Design (cross-cutting)

### Motivation
Common tasks must take one call with safe defaults. Experts must still be able to tune every parameter, without a second API to learn.

### Technical Specification & Design
* **Layer A (need):** one call with safe defaults, e.g. `Crypto.NewKeyPair()`, `Crypto.HashPassword(pw)`. Certificate factories are 0.6.17.
* **Layer B (fine tuning):** the same operation takes an options `STRUCT` with initializers. The 0.6 `STRUCT` already has "named fields, each with a type and an optional initializer" and bracket literals. The default lives in the STRUCT, and the user changes only the field they need. Entities are classes with `STATIC` factories (pattern O4 in `0.6.md`).
* **Naming without overloading:** the spec has no overloading, optional parameters or named arguments. So: pairs `X()` and `XWith(opts)`, or a single `X(opts)` where `opts` is a default-constructed STRUCT.
* **Algorithm selection:** `ENUM`/CONST (`Crypto.Alg.ED25519`) instead of strings, as in BNMath item M5.
* **New types, this close:** `SecretKey` (symmetric), `PrivateKey`, `PublicKey`, `KeyPair` (STRUCT with `PublicKey` and `PrivateKey`). Opaque, zeroized on release, not printable. A key carries its algorithm and a KeyId (SHA-256 of the SPKI). `Bytes` stays the data type. `Certificate` and `CertChain` are 0.6.14b, after CRY-1. `Csr` and `Sealed` are 0.6.17. CRY-2 does not name any of them.
* **Convention below:** **[A]** = need layer; **[B]** = options. Everything returns `T OR Error` unless stated otherwise.

---

## Part 2: Randomness, Secrets and Memory (MVP 1)

### Randomness (OS CSPRNG: getrandom/OsRng)
* [A] `RandomBytes(n) AS Bytes`
* [A] `RandomInt(min, max) AS INTEGER` (unbiased, by rejection)
* [A] `RandomToken(nBytes = 32) AS STRING` (base64url)
* [A] `RandomUuidV4()` and `UuidV7()`
* [A] `RandomPassword(opts AS PasswordGenOptions)` (length 20, character classes)
* The CSPRNG never accepts a seed. Tests use an injected provider, as with `HOST.Clock`.
* `bncrypto.md` already forbids deriving nonces from `HOST.Random` and points at `GenerateNonce` and `RandomBytes`. Do not put the old sentence back (RNG-2).

### Secrets and memory
* `ConstantTimeEquals(a, b) AS BOOLEAN`
* Automatic zeroization on release of `Bytes`/`SecretKey`/`PrivateKey`, plus an explicit `Zeroize()`.
* A `Secret` type (sensitive string) prints `"***"` and cannot be concatenated.
* `mlock` applies to `SecretKey` and `PrivateKey` only. `Bytes` is wiped on release and is not locked: BNWeb stores response bodies in `Bytes`, and locking every body would exhaust `RLIMIT_MEMLOCK`. When the platform cannot lock, key generation still succeeds and the fact is not a silent weakening of the algorithm.
* `PRINT` of a sensitive type is a static error, or shows only the KeyId. `ToHex` stays on `Bytes` and is not added to `SecretKey` or `PrivateKey`.

---

## Part 3: Symmetric Keys and AEAD (MVP 2)

### Symmetric keys
* [A] `NewSecretKey() AS SecretKey` (256 bits, for AEAD)
* [B] `NewSecretKeyWith(KeyOptions{bits: 128|192|256, purpose: AEAD|MAC|KDF, alg})`
* Import/export: `SecretKey.FromBytes(b, purpose)`, `.Export(format: RAW|HEX|BASE64)`. Export requires an explicit `ExportSecret`. JWK waits for 0.6.17.
* Key wrap: `WrapKey(kek, key)` / `UnwrapKey` (AES-KW, RFC 3394, or AEAD).
* `key.Id()`, `key.Algorithm()`, `key.Zeroize()`.

### Symmetric encryption (AEAD)
* [A] `Encrypt(key, plaintext AS Bytes) AS Bytes`: a sealed envelope with the nonce generated internally.
* [A] `Decrypt(key, sealed) AS Bytes`
* [A] `EncryptText(key, STRING) AS STRING` (base64url) and `DecryptText`
* [A] `EncryptFile(key, inPath, outPath)` / `DecryptFile`, streaming (STREAM, 64 KiB segments, as in age). The output file is `FS.OpenNew` from 0.6.12. A failed decrypt deletes that partial file.
* [A] `EncryptFileWithPassword(in, out, password)` / `DecryptFileWithPassword`: Argon2id or scrypt, then XChaCha in segments. New output is `FS.OpenNew`. `EncryptFileFor` (multiple public keys) is 0.6.17.
* [B] `AeadOptions{alg: XCHACHA20_POLY1305 (default, random 24-byte nonce)|AES_256_GCM|AES_256_GCM_SIV|CHACHA20_POLY1305|AES_128_GCM, aad AS Bytes, keyId AS BOOLEAN}`
* **Versioned envelope format:** `magic "BNC1" | version | alg id | keyId (optional) | nonce | ciphertext | tag`. The header is included in the AAD.
* Streaming: `Encryptor.New(key, opts)`, `.Update`, `.Finish`. `Decryptor` releases only data that has been authenticated, segment by segment.
* The current `SealAesGcm`/`SealChaCha20` with a manual nonce stay as a "raw" layer documented as dangerous. With GCM and random 96-bit nonces, recommend rotating the key after 2^32 messages.
* Forbidden: ECB, CBC without MAC, CTR without MAC, RC4, DES/3DES. CBC+HMAC only under `allowLegacy` and only to decrypt.

---

## Part 4: Hash, MAC, Passwords, KDF and Encoding (MVP 3–4)

### Hash
* [A] `Hash(data AS Bytes) AS Bytes` (default SHA-256), `HashText(STRING) AS STRING` (hex)
* [A] `HashFile(path) AS Bytes` (streaming)
* [B] `HashWith(data, HashOptions{alg: SHA256|SHA384|SHA512|SHA512_256|SHA3_256|SHA3_512|BLAKE2b|BLAKE3, outLen})`
* [B] Streaming: `Hasher.New(alg)`, `.Update(bytes)`, `.Finish()`
* XOF: `Shake128`/`Shake256(data, outLen)`
* Legacy (only with `allowLegacy`): MD5 and SHA-1, for checksums only and never for signatures.
* The current `SHA256`/`SHA512` stay as shortcuts.

### MAC
* [A] `Mac(key AS SecretKey, data) AS Bytes` (HMAC-SHA-256)
* [A] `VerifyMac(key, data, tag) AS BOOLEAN` (constant time)
* [B] `MacOptions{alg: HMAC_SHA256|HMAC_SHA384|HMAC_SHA512|BLAKE3_KEYED|KMAC128|KMAC256|POLY1305 (one-time key)}`
* Streaming: `MacStream.New(key, opts)`, `.Update`, `.Finish`, `.Verify`

### Passwords and KDF
* [A] `HashPassword(pw AS STRING) AS STRING`: a PHC string using Argon2id (m=19456 KiB, t=2, p=1, the OWASP minimum) with a random 16-byte salt.
* [A] `VerifyPassword(pw, phc) AS BOOLEAN` (constant time)
* [A] `NeedsRehash(phc, opts) AS BOOLEAN`
* [B] `PasswordOptions{alg: ARGON2ID|SCRYPT|PBKDF2_SHA256, memoryKiB, iterations, parallelism, saltLen, pepper AS SecretKey OR NULL}`. PBKDF2 needs at least 600k iterations. bcrypt is accepted only to verify old hashes.
* [A] `DeriveKey(pw, salt) AS SecretKey`: a key derived from a password (Argon2id).
* [A] `Hkdf(ikm, salt, info, len) AS SecretKey`, plus `HkdfExtract` and `HkdfExpand` (SHA-256/384/512). A length that is not a key this module accepts is `INVALID_ARGUMENT`. The output is not `Bytes`.
* `DeriveSubkey(master AS SecretKey, context AS STRING) AS SecretKey` (HKDF with a label).

### Encoding
* `ToBase64`, `FromBase64`, `ToBase64Url`, `FromBase64Url`, `ToHex` (exists), `FromHex` (exists), `ToPem(label, der)`, `FromPem`, `Concat`, `Slice` (exists), `bytes.ToText()` (strict UTF-8, with Error).

### Non-cryptographic checksums
* `Crc32`, `Crc32c`, `Adler32` and `XxHash64`, kept apart under `Checksum.*` and documented as offering no security.
* `VerifyFileChecksum(path, "sha256:…")`

---

## Part 5: Asymmetric Key Pairs and Signatures (MVP 5)

### Key pairs
* [A] `NewKeyPair() AS KeyPair` (default Ed25519)
* [A] `NewKeyPairFor(alg AS Alg)`: Ed25519, X25519, and ECDSA P-256. ED448, P-384, P-521, RSA, ML-KEM-1024, ML-DSA-87, SLH-DSA, and the hybrid KEM are 0.6.17. Do not add those algorithm names in the MVP spec.
* [B] `NewKeyPairWith(KeyPairOptions{alg, label})`. No seed field, no `rsaBits`, no `rsaExponent`. Those RSA fields arrive with 0.6.17. Tests inject the CSPRNG provider from Part 2. A seed argument in the language API would ship a backdoor.
* Private export: `priv.ExportPem(format, password)` always encrypts. `priv.ExportPemUnencrypted(format)` is the separate function that writes a plaintext key. A null password is not the switch. Encrypted PKCS#8 uses PBES2 with Argon2id or scrypt. The password does not appear in `Error.Message` or `Error.Cause`. OpenSSH export is 0.6.17.
* Public export: `pub.ExportPem()` (SPKI), `pub.Fingerprint(SHA256)`. `ExportJwk` and `ExportOpenSsh` are 0.6.17.
* Import: `LoadPrivateKey(text OR Bytes, password OR NULL)`. Format detection accepts only an unambiguous header. Ambiguous input is `PARSE_ERROR`, not a guess. `LoadPublicKey` is separate.
* `kp.Public()`, `priv.Algorithm()`, `priv.Id()`
* Replace concatenation+`Slice` with STRUCT results everywhere (problem 5).

### Signatures
* [A] `Sign(priv, message) AS Bytes` and `Verify(pub, message, sig) AS BOOLEAN`. The algorithm comes from the key.
* [A] `SignFile(priv, path)` and `VerifyFile`
* [B] `SignOptions{hash (ECDSA P-256), encoding: RAW|DER, context (Ed25519ph), prehash}`. `rsaPadding` and ML-DSA context wait for 0.6.17.
* Detached signatures, in an own format or compatible with minisign/SSHSIG: `SignDetached` / `VerifyDetached`.

---

## Not this close

`LoadCertificate` and `VerifyChain` are `todo/bucket-0.6.14b-crypto-pki.md`. Issuance, key agreement, RSA above the MVP, JOSE, OTP, SSH, and the later rows (CRL, KeyStore, age, SSH certificates, hybrid KEM, ML-DSA-87, SLH-DSA, JWE/PASETO) are `todo/bucket-0.6.17-crypto-pki-features.md`. They are not activities of this file. `EncryptFile` and `EncryptFileWithPassword` stay in Part 3. ML-KEM and ML-DSA KATs for the algorithms that already ship stay in KAT-1.

0.6.15 uses `Bytes`, `Hash`, `HashFile`, `ToText`, and `ConstantTimeEquals`. It does not use `Certificate`. `Trust`, `WithClientIdentity`, and `FetchCertificate` are `todo/bucket-0.6.15b-bnweb-client-identity.md`. Server `TLSConfig.FromPEM` is already shipped.

---

## Part 8: Policy and Error Model (this close)

* `Crypto.Policy{allowLegacy FALSE, fipsOnly FALSE}` is an argument of the call that needs it. There is no process-wide switch and no environment variable. `allowLegacy` does not survive the call. `minRsaBits` is not a field until 0.6.17 introduces RSA above the MVP.
* New errors, this close: `UNSUPPORTED_ALGORITHM`, `LEGACY_DISABLED`, `PARSE_ERROR`, `WRONG_KEY_TYPE`, `IO_ERROR`, `WRONG_PASSWORD`. `CHAIN_INVALID`, `EXPIRED`, `NOT_YET_VALID`, and `REVOKED` wait for 0.6.14b.
* `Verify` and `VerifyMac` keep returning BOOLEAN. `VerifyChain` is 0.6.14b.
* **Always forbidden in this close:** ECB, RC4, DES, MD5/SHA-1 in signatures, fixed nonces, a seeded CSPRNG in production. RSA floors, PKCS#1 v1.5 encryption, and `alg=none` are 0.6.17, because those algorithms are not in the MVP.
* **Only with `allowLegacy`, and only to read or verify:** MD5 and SHA-1 as checksums, CBC+HMAC decrypt, bcrypt verification, PBKDF2 with few iterations. TOTP SHA-1 and PKCS#12 RC2/3DES are 0.6.17.

---

## Examples (proposed)

```basic
IMPORT BNCrypto AS Crypto

' 1. Symmetric key + encrypt text (internal nonce, versioned envelope)
LET key AS Crypto.SecretKey OR Error = Crypto.NewSecretKey()
IF key IS Crypto.SecretKey THEN
    LET token AS STRING OR Error = Crypto.EncryptText(key, "segredo")
END IF

' 2. Key pair with a chosen algorithm, in one call
LET kp AS Crypto.KeyPair OR Error = Crypto.NewKeyPairFor(Crypto.Alg.ED25519)

' 3. The same with fine tuning
LET ko AS Crypto.KeyPairOptions
ko.alg = Crypto.Alg.ECDSA_P256
LET p256 AS Crypto.KeyPair OR Error = Crypto.NewKeyPairWith(ko)

' 4. Password: hash and verify
LET phc AS STRING OR Error = Crypto.HashPassword(pw)
IF Crypto.VerifyPassword(tentativa, phc) THEN PRINT "ok"

' 5. Sign
LET sig AS Crypto.Bytes OR Error = Crypto.Sign(kp.PrivateKey, msg)
```

Certificate, HPKE, and RSA examples are in `todo/bucket-0.6.17-crypto-pki-features.md`.

Open point: STRUCT field-assignment syntax and `Crypto.Alg.X` as an ENUM must be checked against the 0.6 grammar (CRY-0).

---

## Activity Checklist

### Part 0–1: Design gate
- [ ] **CRY-0 (Grammar check):** Confirm STRUCT field assignment, STRUCT initializers/bracket literals, and `ENUM`/CONST namespacing for the MVP algorithms (`Crypto.Alg.X`) against `language/0.6/0.6.md`; choose `X()`+`XWith(opts)` vs single `X(opts)`. `Crypto.Eku` waits for 0.6.17.
- [ ] **CRY-2 (Spec):** Rewrite `language/0.6/bncrypto.md` around the two layers and the MVP types (`SecretKey`, `PrivateKey`, `PublicKey`, `KeyPair`), KeyId, and the MVP error model. Do not add `Certificate`, `Csr`, `CertChain`, or `Sealed` in this rewrite. `Certificate` and `CertChain` belong to 0.6.14b, after CRY-1. `Csr` and `Sealed` belong to 0.6.17.

### Part 2: Randomness & memory (MVP 1)
- [ ] **RNG-1 (CSPRNG):** Keep the existing `getrandom` path. Add `RandomInt`, `RandomToken`, `RandomUuidV4`, `UuidV7`, and `RandomPassword` on it. No seed and no `HOST.Random`. `RandomToken` and `RandomPassword` reject a length of 0. `RandomInt` is unbiased rejection sampling. Tests inject a provider.
- [ ] **RNG-2 (Spec fix):** Confirm `bncrypto.md` keeps the `HOST.Random` prohibition. The old "derive nonces from `HOST.Random`" sentence is already gone; do not put it back.
- [ ] **MEM-1 (Zeroize):** `Key` already wipes. Wipe `Bytes` on the last `RELEASE`, and wipe the temporary buffers in `random_bytes`, `seal`, `open`, `hmac_sha256`, and the ML-KEM/ML-DSA seed copies after they are installed in the destination. Explicit `Zeroize()` stays.
- [ ] **MEM-2 (Secrets):** `ConstantTimeEquals`; `Secret` type printing `"***"` and non-concatenable; `PRINT` of sensitive types is a static error (or KeyId only). `mlock` only for `SecretKey` and `PrivateKey`.

### Part 3: Symmetric (MVP 2)
- [ ] **SYM-1 (SecretKey):** `NewSecretKey`, `NewSecretKeyWith(KeyOptions)`, `FromBytes`, `Export` gated by `ExportSecret`, `Id`, `Algorithm`, `Zeroize`, `WrapKey`/`UnwrapKey`.
- [ ] **SYM-2 (Envelope):** `Encrypt`/`Decrypt`/`EncryptText`/`DecryptText` with the `BNC1` versioned envelope (header in AAD), XChaCha20-Poly1305 default, `AeadOptions`.
- [ ] **SYM-3 (Raw layer docs):** Keep `SealAesGcm`/`SealChaCha20` documented as dangerous; 2^32-message rotation note; forbidden modes list.
- [ ] **FILE-1:** `EncryptFile` / `DecryptFile` and `EncryptFileWithPassword` / `DecryptFileWithPassword`. Paths go through `HOST.FileSystem`. New output is `FS.OpenNew`, not `FS.Open` `WRITE` and not a copy of the source mode. A failed decrypt deletes the partial output.

### Part 4: Hash, MAC, passwords, encoding (MVP 3–4)
- [ ] **HSH-1:** `Hash(Bytes)`, `HashText`, `HashFile` (streaming), `HashWith` (SHA-384/512-256, SHA3, BLAKE2b, BLAKE3), `Hasher`, SHAKE; `SHA256`/`SHA512` kept as shortcuts; legacy MD5/SHA-1 behind `allowLegacy`.
- [ ] **MAC-1:** `Mac`/`VerifyMac` over `SecretKey`, `MacOptions`, `MacStream`.
- [ ] **PWD-1:** `HashPassword` (PHC Argon2id, m=19456, t=2, p=1, 16-byte salt), `VerifyPassword`, `NeedsRehash`, `PasswordOptions` (scrypt, PBKDF2 ≥ 600k, pepper; bcrypt verify-only).
- [ ] **KDF-1:** `DeriveKey`, `Hkdf`/`HkdfExtract`/`HkdfExpand` (SHA-256/384/512), `DeriveSubkey`.
- [ ] **ENC-1:** Base64/Base64Url/PEM codecs, `Concat`, strict `ToText`; `Checksum.*` (Crc32, Crc32c, Adler32, XxHash64) and `VerifyFileChecksum`.

### Part 5: Asymmetric (MVP 5)
- [ ] **ASY-1 (Types):** `PrivateKey`/`PublicKey`/`KeyPair` STRUCT; replace concatenation+`Slice` in ML-KEM/ML-DSA/Ed25519/ECDSA results.
- [ ] **ASY-2 (Generation):** `NewKeyPair`, `NewKeyPairFor` (Ed25519, X25519, P-256 in the MVP), `NewKeyPairWith(KeyPairOptions)`.
- [ ] **ASY-3 (Generic sign):** `Sign`/`Verify`, `SignFile`/`VerifyFile`, `SignOptions`, `SignDetached`/`VerifyDetached`.
- [ ] **ASY-4 (Import/export):** PKCS#8/SPKI PEM (MVP) and password-protected export. `LoadPrivateKey` accepts only an unambiguous header. Ambiguous input is `PARSE_ERROR`. No auto-detection. `LoadPublicKey` is separate. OpenSSH, JWK, and PKCS#12 stay in 0.6.17.

### Cross-cutting, this close
- [ ] **POL-1 (Policy):** `Crypto.Policy{allowLegacy, fipsOnly}` is an argument of the call that needs it. There is no process-wide switch and no environment variable. `allowLegacy` does not survive the call. `minRsaBits` waits for 0.6.17, which is where RSA above the MVP is introduced.
- [ ] **ERR-1 (Errors):** Add `UNSUPPORTED_ALGORITHM`, `LEGACY_DISABLED`, `PARSE_ERROR`, `WRONG_KEY_TYPE`, `IO_ERROR`, and `WRONG_PASSWORD`. Chain and validity codes wait for 0.6.14b (`VerifyChain`).
- [ ] **KAT-1 (PQC vectors):** Local KATs for ML-KEM and ML-DSA. Include one ML-KEM implicit-rejection vector: a well-sized wrong ciphertext does not return `AUTHENTICATION_FAILED`. `bncrypto.md` currently says `MlKemDecapsulate` rejects the ciphertext with that code. The code returns a secret for any ciphertext of the right length (`ml_kem_decapsulate`). Correct the spec to match implicit rejection. Do not "fix" the function into an error.
- [ ] **TST-1 (Parity):** Positive and fail-closed fixtures on `bni` and `bnc` for every new function; independent vectors; Linux/macOS/Windows.
- [ ] **DOC-1 (Book):** Update Appendix K with the MVP API only, after 0.6.11 S4.2. Do not document certificates, JWT, or OTP. The read path is 0.6.14b. JWT and OTP are 0.6.17.

### Security corrections (2026-10-28)

The current module is fail-closed on AEAD tags, HMAC comparison (`subtle::ct_eq`), P-256 rejection sampling, and ML-KEM encapsulation randomness. The rows above that only restated the inventory are not repeated here.

- [ ] **CRY-SEC-1 (Inventory):** Part 0 items 1 and 3 are the inventory. `getrandom` and `Key` wipe stay. Do not restore a claim that the CSPRNG or `zeroize` is absent, and do not restore a nonce sentence that tells the caller to use `HOST.Random`.
- [ ] **CRY-SEC-2 (No seeded keygen):** `KeyPairOptions` has no seed. `ExportPem` encrypts; plaintext export is `ExportPemUnencrypted`. `LoadPrivateKey` does not guess an ambiguous encoding.
- [ ] **CRY-SEC-4 (Policy and files):** `allowLegacy` and `fipsOnly` are per call. `HashFile`, `EncryptFile`, and `DecryptFile` use `HOST.FileSystem` and `FS.OpenNew` for new output. The existing `Argon2id` function keeps its current cost floor so `examples/sqlite.bn` and `tests/modules/bncrypto-mac-kdf` still run; `HashPassword` is the password API and it uses the OWASP floor. Do not raise the old function's floor in this bucket. CSR, JWT, PKCS#12, and TOTP rules are activities of 0.6.17, not of this close.

Residuals, no change in this bucket. `bn_rt_crypto_sha256` and `bn_rt_crypto_sha512` return `""` for a null or non-UTF-8 pointer; that string is not a digest. `hmac_verify` compares lengths before `ct_eq`; the tag length is public. A well-sized ML-KEM ciphertext always yields a shared secret (implicit rejection). Do not turn that into `AUTHENTICATION_FAILED`. `bncrypto.md` still says otherwise; KAT-1 corrects the spec.
