# Bucket 0.6.15b — BNWeb Client Certificate Identity

**Status:** todo, split from `todo/bucket-0.6.15-bnweb-transfer-and-quic.md` on 2026-10-28. Not part of the 0.6.15 close.

**Starts after:** 0.6.15 and 0.6.14b (`LoadCertificate`, `VerifyChain`, `PrivateKey` from the 0.6.14 MVP). It does not wait for 0.6.16 or for 0.6.17. If CRY-1 is no, this bucket does not start. When CRY-1 is yes before 0.6.16 starts, this bucket runs with 0.6.14b, before that language change. `SystemTrust`, pins, and `NewUnverified` stay in 0.6.15 and do not wait. 0.6.13 is unassigned.

## Activities

- [ ] **WEB-7b (Spec and provider):** `Trust` and `AddTrust` take `Crypto.Certificate`. `WithClientIdentity` takes `Crypto.Certificate` and `Crypto.PrivateKey`. `FetchCertificate(url) AS Crypto.Certificate` returns the peer leaf of a verified handshake. Parse errors stay BNCrypto errors. No provider path runs before the spec text.
- [ ] **WEB-8b (Fixture):** `Trust` of one certificate rejects a peer signed by another. Both backends, no public network.
