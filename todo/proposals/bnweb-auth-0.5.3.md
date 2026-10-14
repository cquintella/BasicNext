# Proposal: BNWeb authentication (0.5.3)

**Status:** Proposed — design locks incomplete; ambition recorded.
**Date:** 2026-09-19
**Target:** **0.5.3** (with S1 BNSqlite + FS ops).
**Gate:** Quorra before Carlos.
**Motivation (Carlos):** BNWeb needs authentication supporting **AES-256**, **NIST-accepted post-quantum** public-key protocols, and **OAuth** integration.

**Related:** [`bncrypto-0.5.2c.md`](bncrypto-0.5.2c.md) (**A0** home); `modules/bn/BNWeb.bn`; `HOST.Net`; session store may use **BNSqlite** (same bucket) — Auth must not invent a second persistence stack silently.

---

## Intent locks (Carlos — to refine)

| Theme | Intent |
| --- | --- |
| Symmetric | AES-256 for confidentiality of tokens/cookies/payloads where symmetric crypto applies |
| Asymmetric / PQ | Public-key protocols already accepted by **NIST** (post-quantum) for handshake / signatures as claimed |
| Federation | **OAuth** integration (version and flows to lock: OAuth 2.0 / OIDC) |
| Fail-closed | Missing crypto / policy deny → Error; no silent plaintext “dev mode” in claimed builds |

---

## Suggested layering (opinion → lock)

| Layer | Role | 0.5.3 spirit |
| --- | --- | --- |
| **A0** | **`BNCrypto`** primitives (see [`bncrypto-0.5.2c.md`](bncrypto-0.5.2c.md)) | AES-256-GCM + ChaCha20-Poly1305 AEAD; SHA-2 hashes; Argon2id for passwords; optional PQ KEM/signature **via** native backend — exact API in BNCrypto proposal |
| **A1** | Session / password / token helpers in BNWeb | Issue/verify session token; password verify (if local accounts); constant-time compare |
| **A2** | OAuth client | Authorization-code (+ PKCE) against external IdP; store tokens via BNSqlite or sealed file |
| **A3** | HTTP binding | Login routes / middleware style hooks on BNWeb handlers — no new language keywords |

**Risk:** Claiming full PQ + OAuth + production session security in one tag without delimitation invites done-oco. Prefer **closed sub-claims**:

1. **A0** primitives + fixtures (must).
2. **A1** local session over HTTPS assumption documented (must for “Auth”).
3. **A2** OAuth 2.0 auth-code+PKCE against one reference IdP (must if OAuth stays in claim).
4. PQ algorithms: **name the NIST selections** (e.g. ML-KEM / ML-DSA family — lock exact names) and one interoperable vector set — or move PQ to **0.5.3b** if F1/SQLite/FS already fill the tag.

## Non-goals (unless expanded)

- Full IdP server (authorize others’ clients).
- GUI login forms (`HOST.Ui`).
- “Roll our own” novel crypto.
- Claiming quantum-safe **everything** if only hybrid or only signatures ship.

## Open questions (block Ready)

1. Which NIST PQ algorithms are **in claim** (exact names + versions)?
2. OAuth: 2.0 only vs OIDC userinfo? Which grant types?
3. Where do sessions live — memory, file, BNSqlite table?
4. ~~HOST.Crypto vs BNCrypto~~ → **BNCrypto** (Carlos 2026-09-19); confirm spelling `BNCrypto` vs `BNCrypto` for the import id.
5. Browser cookie vs Bearer header teaching default?

## Acceptance

No tag until each **locked** A-row has fixtures + book/library text + Quorra gate. Unlocked PQ/OAuth rows must be cut or deferred in writing.


## History

- **2026-09-19** — Carlos: BNCrypto/BNCrypto as aparte module (direction B style); Auth consumes it.

**Retarget (2026-09-21, invert):** claim lives in `todo/bucket-0.6.3.md` (was 0.5.3); depends on BNCrypto **0.6.1**.
