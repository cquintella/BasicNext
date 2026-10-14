# Proposal: `BNCrypto` module with native backend — bucket 0.5.2c

**Retarget (2026-09-22):** delivery bucket is **0.6.1b** (`ongoing/bucket-0.6.1b.md`). Version **0.6.1** is the closed language bucket for `++`/`--` in `done/`, not this module.

**Status:** Proposed.
**Module id:** **`BNCrypto`** (English; Carlos 2026-09-19).
**Bucket:** [`ongoing/bucket-0.5.2c.md`](../../ongoing/bucket-0.5.2c.md)
**Date:** 2026-09-19
**Gate:** Quorra before Carlos.
**Pattern:** aparte module + native backend (same honesty rule as `BNSqlite` / F1). Not primary `HOST.Crypto`.

**Consumers later:** BNWeb Auth (0.5.3), apps, token sealing — they **import** `BNCrypto`; they do not reimplement primitives.

---

## Claim — algorithms in 0.5.2c (closed set)

Nothing outside this table ships as “BNCrypto done.” Rows marked **cut** stay out until a later bucket.

### Symmetric / AEAD (cifras no sentido estrito)

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **S1** | **AES-256-GCM** | `Seal` / `Open` (key, nonce, plaintext, AAD) → ciphertext `OR Error` | Confidentiality + integrity (tokens at rest, app payloads) |
| **S2** | **ChaCha20-Poly1305** | same shape | Portable AEAD when AES-NI is weak/absent; TLS-family peer |

### Hash (not password storage)

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **H1** | **SHA-256** | `Digest(data) AS bytes OR Error` | Fingerprints, HKDF input, integrity |
| **H2** | **SHA-512** | same | Same family |

### MAC

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **M1** | **HMAC-SHA-256** | `Mac(key, data)` / `Verify` | Cookie/token integrity (symmetric) |

### Password KDF (slow, memory-hard)

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **P1** | **Argon2id** | `Hash(password, salt, params)` / `Verify` | Local password storage (OWASP default) |

### Public-key signatures (classical)

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **K1** | **Ed25519** | `Keygen` / `Sign` / `Verify` | Preferred JWT/`EdDSA`-class, new systems |
| **K2** | **ECDSA P-256 + SHA-256** (JOSE **ES256**) | `Sign` / `Verify` | Interop with common OAuth/OIDC IdPs |

### Post-quantum (NIST — exact names locked)

| ID | Algorithm | API spirit | Role |
| --- | --- | --- | --- |
| **Q1** | **ML-KEM-768** (FIPS 203 / Kyber) | `Keygen` / `Encaps` / `Decaps` | PQ key encapsulation |
| **Q2** | **ML-DSA-65** (FIPS 204 / Dilithium) | `Keygen` / `Sign` / `Verify` | PQ signatures |

### Explicitly **out** of 0.5.2c (unless Carlos expands)

- AES-128-GCM only as internal TLS concern (app claim is **AES-256-GCM**)
- AES-CBC / CTR without AEAD; 3DES; RC4; Blowfish
- MD5 / SHA-1 for security use
- bcrypt / scrypt / PBKDF2 as **primary** (Argon2id only in claim; legacy verify may be follow-on)
- RSA-PKCS1v1.5 sign (**RS256**) — interop follow-on if an IdP forces it
- Full TLS stack / X.509 (stays OS / `HOST.Net` / reverse proxy)
- OAuth protocol (0.5.3 Auth consumes BNCrypto)

### Randomness

Use **`HOST.Random`** (or BNCrypto thin `Fill` that calls it) for keys/nonces — no second RNG policy.

---

## Native wire

**F1c:** minimum native binding so each claimed algorithm is honest (no pure-`.bn` theater). Backend choice (e.g. RustCrypto + liboqs / system) is implementation detail; support matrix must name it.

## Acceptance

Fixtures per row S1–S2, H1–H2, M1, P1, K1–K2, Q1–Q2 (positive + fail-closed). Interpret green; native parity if claimed. Book/library. Quorra. Release note under the 0.5.2 line or patch note per tag policy in the bucket.

**Retarget (2026-09-21, invert):** claim lives in `todo/bucket-0.6.2.md` (was 0.5.2c).

**Retarget (2026-09-21, invert):** claim in `todo/bucket-0.6.1.md`.
