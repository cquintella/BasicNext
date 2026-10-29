# Bucket 0.6.17 — BNCrypto PKI Features

**Status:** todo. Renumbered from 0.6.14c on 2026-10-28 because it needs `STRING[]` from 0.6.16. Split from `todo/bucket-0.6.14b-crypto-pki.md`. Not part of the 0.6.14 or 0.6.14b close. Not a dependency of 0.6.15 or 0.6.15b.

**Gate:** CRY-1, already recorded for 0.6.14b. A refusal there ends this bucket. This file does not start a second product inside the fundamentals close.

**Starts after:** 0.6.14b and 0.6.16. Issuance takes `STRING[]` (`NewServerCertificate`, SAN, EKU) and `EncryptFileFor` takes `PublicKey[]`. Those types do not exist before 0.6.16. When this bucket starts, it closes before 0.7.0. A CRY-1 refusal skips it and does not block 0.7.0. 0.6.13 is unassigned.

`FS.Open` is unchanged. New private key files and PKCS#12 output use `FS.OpenNew`.

## Activities

- [ ] **PKI-1:** `NewSelfSignedCertificate`, `NewCertificateAuthority`, `NewUserCertificate`(`With`), `NewServerCertificate`, `NewClientCertificate`, `NewIntermediateCa`, `CertOptions`. A server certificate whose `validDays` exceeds 398 is `INVALID_ARGUMENT`. The easy server call stays 397 days. Reject, do not clamp. `SignCsr` does not copy SAN, EKU, or basic constraints from the CSR. `San()` as `STRING[]` lands here.
- [ ] **PKI-2:** `NewCsr`, `LoadCsr`, `SignCsr`, PKCS#12. PKCS#12 is AES-256 and PBKDF2. RC2 and 3DES import only under `allowLegacy`, inside a size cap. The password is not copied into an `Error`. Output uses `FS.OpenNew`.
- [ ] **HPKE-1:** `SealFor` / `OpenSealed`, `EncryptFileFor`, `HpkeOptions`, `KeyAgreement` as `SecretKey`, `KemResult{ciphertext AS Bytes, secret AS SecretKey}`. RSA-OAEP for interop. PKCS#1 v1.5 encryption stays forbidden.
- [ ] **RSA-1:** RSA 3072/4096 (PSS/OAEP) and ECDSA P-384. `minRsaBits` below 2048 is `INVALID_ARGUMENT`. `KeyPairOptions` gains `rsaBits` (default 3072) and `rsaExponent` 65537 here, not in the 0.6.14 MVP.
- [ ] **JOSE-1:** `NewJwt` / `VerifyJwt` (allowlist, no `alg=none`, no HS/RS confusion), JWS Compact, JWK, `LoadJwks(text)` only, thumbprint. Reject `crit`. A nested token is not a JWS. No HTTP client.
- [ ] **OTP-1:** TOTP and HOTP compared in constant time, window ±1, wrong length is `FALSE`. The secret is a `Secret`. `TotpUri` may be a `STRING` and is not logged. Recovery codes are single-use in the value the program stores.
- [ ] **SSH-1:** `NewSshKey`, OpenSSH export and import, `ExportAuthorizedKey`, `SshFingerprint`. A new private key file is `FS.OpenNew`.
- [ ] **LATER-1:** CRL/OCSP, KeyStore, PKCS#11, age, SSH certificates, hybrid KEM, ML-DSA-87, SLH-DSA, JWE/PASETO. Not required to close PKI-1. Excluded from this close if still open when PKI-1 through SSH-1 pass; move the unfinished rows forward rather than announcing them done.
- [ ] **DOC-1c:** Appendix K gains factories, JWT, and OTP only after those fixtures pass.

The activity list is the plan. The design below was moved out of 0.6.14 and then out of 0.6.14b. Where the two disagree, the activity list wins. `EncryptFile` and `EncryptFileWithPassword` stay in 0.6.14. `LoadCertificate` and `VerifyChain` stay in 0.6.14b.

## Key agreement and public-key encryption

* [A] `SealFor(recipientPub, plaintext) AS Bytes` / `OpenSealed(recipientPriv, sealed)`, using HPKE (RFC 9180) DHKEM-X25519 + HKDF-SHA256 + ChaCha20Poly1305. The hybrid KEM X25519+ML-KEM-768 is LATER-1.
* [A] `EncryptFileFor(pubs AS PublicKey[], in, out)`: multiple recipients, age style. Output is `FS.OpenNew`.
* [B] `HpkeOptions{kem, kdf, aead, mode: BASE|AUTH|PSK, info, aad}`
* Raw: `KeyAgreement(priv, peerPub) AS SecretKey` (X25519/ECDH, passed through HKDF). `Encapsulate(pub)` / `Decapsulate(priv, ct)` return `STRUCT KemResult{ciphertext AS Bytes, secret AS SecretKey}`. Neither result is a concatenated `Bytes` buffer with `ToHex`.
* RSA-OAEP (SHA-256) for interoperability only. PKCS#1 v1.5 encryption is forbidden.

## Issuance

* [A] `NewSelfSignedCertificate(commonName) AS CertBundle` (key + certificate)
* [A] `NewCertificateAuthority(name) AS CertBundle`: root CA, `pathLen`, 10 years
* [A] `NewUserCertificate(name, email, issuer AS CertBundle) AS CertBundle`: EKU clientAuth+emailProtection, SAN rfc822
* [A] `NewServerCertificate(dnsNames AS STRING[], issuer)`: EKU serverAuth, SAN DNS/IP, 397 days. `validDays` above 398 is `INVALID_ARGUMENT`. Reject, do not clamp.
* [A] `NewClientCertificate(id, issuer)`
* [A] `NewIntermediateCa(name, issuer)`
* [B] `CertOptions{keyAlg (ECDSA_P256 default), subject AS DistinguishedName{CN,O,OU,C,ST,L,email}, san AS STRING[] (dns:/ip:/email:/uri:), validFromNow, validDays, notBefore, notAfter, serial (random 128-bit), keyUsage[], extKeyUsage[], isCa, pathLen, crlUrls, ocspUrl, policies, existingKey AS KeyPair OR NULL, signatureHash}`, used through `New…CertificateWith(…, opts)`.
* CSR: `NewCsr(keyPair, CsrOptions)`, `LoadCsr(pem)`, `SignCsr(csr, issuer, CertOptions)`. `SignCsr` does not copy SAN, EKU, or basic constraints from the CSR. Those fields come only from `CertOptions`.
* `San()` as `STRING[]` is this bucket. 0.6.14b returns names through `Describe` and the single-line `Subject` / `Issuer` strings.
* PKCS#12: `ExportPkcs12(bundle, password)` / `LoadPkcs12(bytes, password)` with AES-256 + PBKDF2. The output file is `FS.OpenNew`. RC2 and 3DES are accepted only on import, under `allowLegacy`, and only inside the size cap. The password is not copied into an `Error`.

## RSA and P-384

RSA 3072/4096 with PSS signatures and OAEP encryption, plus ECDSA P-384, wired into `NewKeyPairFor`, `Sign`/`Verify`, and the raw layer of `SealFor`. `minRsaBits` below 2048 is `INVALID_ARGUMENT`.

## JOSE

* [A] `NewJwt(claims AS Json, key) AS STRING`, with `iat`/`exp` already set.
* [A] `VerifyJwt(token, key, JwtOptions{issuer, audience, leewaySec, allowedAlgs})`. It rejects `alg=none` and HS/RS confusion, and requires an allowlist.
* JWS Compact. A nested token is not a JWS. Reject `crit`.
* JWK: `ExportJwk`, `LoadJwk`, `LoadJwks(text)`, `Thumbprint` (RFC 7638). `LoadJwks` does not fetch a URL. This module does not grow an HTTP client.
* Algorithms: EdDSA, ES256/384, PS256, RS256 (interop), HS256.

## OTP

* [A] `NewTotpSecret() AS Secret` (base32 inside the secret). `TotpUri(secret, issuer, account)` may return a `STRING` and is not logged. `TotpCode(secret)`, `VerifyTotp(secret, code, TotpOptions{digits 6, period 30, alg SHA1 (RFC 6238 compat)|SHA256, window ±1})`. Comparison is constant time. A wrong length is `FALSE`.
* HOTP: `HotpCode(secret, counter)` and `VerifyHotp`. The secret is a `Secret`, not a `STRING`.
* Recovery codes: `NewRecoveryCodes(n)`, single-use in the value the program stores.

## SSH keys

* `NewSshKey(comment, alg = ED25519)`
* `priv.ExportOpenSsh(password)`, `pub.ExportAuthorizedKey()`, `LoadSshKey`
* `SshFingerprint` (SHA256:…)

## Later (LATER-1)

Not required to close PKI-1. If still open when PKI-1 through SSH-1 pass, move the unfinished rows forward.

* Revocation: `NewCrl`, `CheckCrl`, `OcspRequest`, `ParseOcspResponse`.
* KeyStore, OS keychain, PKCS#11, TPM.
* age file format.
* SSH certificates: `SignSshCertificate`.
* Hybrid KEM, ML-DSA-87, SLH-DSA, and their KATs. The KATs for ML-KEM-768 and ML-DSA-65 stay in 0.6.14 KAT-1.
* JWE and PASETO.

## Example

```basic
IMPORT BNCrypto AS Crypto

LET ca AS Crypto.CertBundle OR Error = Crypto.NewCertificateAuthority("Intelliurb Root CA")
LET user AS Crypto.CertBundle OR Error = Crypto.NewUserCertificate("Ana Souza", "ana@intelliurb.com", ca)

LET co AS Crypto.CertOptions
co.keyAlg = Crypto.Alg.ED25519
co.validDays = 90
co.san = ["email:ana@intelliurb.com"]
co.extKeyUsage = [Crypto.Eku.CLIENT_AUTH]
LET user2 AS Crypto.CertBundle OR Error = Crypto.NewUserCertificateWith("Ana Souza", ca, co)

LET box AS Crypto.Bytes OR Error = Crypto.SealFor(bobPub, Crypto.FromText("oi Bob"))
```
