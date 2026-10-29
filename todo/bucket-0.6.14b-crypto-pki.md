# Bucket 0.6.14b — BNCrypto Certificate Fundamentals

**Status:** todo, split from `todo/bucket-0.6.14-crypto-enhancements.md` on 2026-10-28 and reduced to the read path on 2026-10-28. Not part of the 0.6.14 close and not a dependency of 0.6.15. Issuance, HPKE, RSA, JOSE, OTP, and SSH are `todo/bucket-0.6.17-crypto-pki-features.md`.

**Gate:** CRY-1. Carlos reopens X.509 material in BNCrypto, which `language/0.6/bncrypto.md` places under "Not here". This reopen is the certificate as a value: load, describe, export, and `VerifyChain`. It does not reopen OAuth, ORM field encryption, `HOST.Ui`, or the TLS stack. RSA-RS256 stays out of this bucket. If the decision is no, this bucket does not start, and neither do 0.6.17 nor `todo/bucket-0.6.15b-bnweb-client-identity.md`.

**Starts after:** the 0.6.14 MVP (`PrivateKey`, `PublicKey`, `Hash`, `ToText`, `FS.OpenNew`). It does not wait for 0.6.16. It uses no `STRING[]` and no `PublicKey[]`. It does not block 0.6.15, 0.6.16, or 0.7.0. When CRY-1 is yes before 0.6.16 starts, this bucket and 0.6.15b run after 0.6.15 and before that language change. 0.6.13 is unassigned.

None of this is closed today. The TLS handshake stays in BNWeb.

## Activities

- [ ] **CRY-1 (BDFL):** Record the reopen, or the refusal, in `bncrypto.md`. Name this bucket as the scope. A refusal ends this bucket, 0.6.17, and 0.6.15b.
- [ ] **PKI-READ-1:** Opaque `Certificate` and opaque `CertChain`. `LoadCertificate(pem OR der)`, `LoadCertChain`. `Subject`, `Issuer`, `NotAfter`, `Serial`, and `Fingerprint` return `STRING`. `PublicKey` returns `PublicKey`. `Describe` returns readable text. `ExportPem` and `ExportDer`. No `San()` of type `STRING[]` in this close. No CA factory, no CSR, no PKCS#12.
- [ ] **PKI-READ-2:** `VerifyChain(leaf, intermediates AS CertChain, roots AS CertChain, VerifyOptions{time, purpose, hostname, useSystemRoots})` returns a STRUCT with the reason. `purpose` is required. A server purpose requires `hostname`. Omitted `time` is `HOST.Clock`. `useSystemRoots` does not disable the hostname check.
- [ ] **ERR-1b:** `CHAIN_INVALID`, `EXPIRED`, `NOT_YET_VALID`, `REVOKED`, `PARSE_ERROR` already named by the MVP stays the parse failure.
- [ ] **DOC-1b:** Appendix K gains only these read and verify operations, after the fixtures pass.

## What stays out

`NewServerCertificate`, `NewCertificateAuthority`, `SignCsr`, `EncryptFileFor`, `SealFor`, RSA, JWK, JWT, TOTP, and OpenSSH are 0.6.17. They are not implemented from the design notes that used to live in this file.
