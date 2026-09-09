# Basic Next 0.4.6 — Security hardening (Track A only)

**Status:** Implementation claimed complete 2026-09-07 (G0–G5). UNUSED_BINDING public-export fix is **not** in this bucket — see [`bucket-0.4.7.md`](bucket-0.4.7.md) §P0. Optional 4a.2 remains non-blocking.

**Objective:** Close the confirmed Rust/HOST security findings from the
2026-09-07 audit with fail-closed behaviour, living BN-SEC authority, and
regression tests. Prefer evidence over doc-only fixes.

**Sibling:** Matrix expansion lives in [`bucket-0.4.7.md`](bucket-0.4.7.md)
(Track B). **Do not** mix matrix checkboxes into this file.

**Release rule (G6 shared):** Version bump / “0.4.6+security+matrix” product
claim is allowed only when **this** bucket’s G0–G5 are Fixed-or-Deferred-with-owner
**and** [`bucket-0.4.7.md`](bucket-0.4.7.md) G7.1–G7.3 are accepted. Shared
release checklist: [`WBS-0.4.6-release.md`](WBS-0.4.6-release.md).

**Relationship:**
- **0.4.5** — CLOSED ([`../done/bucket-0.4.5.md`](../done/bucket-0.4.5.md)).
- **0.4.6** — *this file* — security only.
- **0.4.7** — LLVM support-matrix expansion for `examples/` ([`bucket-0.4.7.md`](bucket-0.4.7.md)).

**Owner convention:** every activity and gate lists **Owner**. Until Carlos
names an implementer, Owner defaults to **Doug** (accountable for evidence and
for refusing to mark Fixed without tests). DEFER requires: Owner, residual risk
(one sentence), and **Defer-until** (date or blocking AQ/bucket id).

**Inputs:**
- Security audit findings F-01…F-10 / residuals R-01…R-03 (2026-09-07).
- [`../docs/architecture/nfr-security.md`](../docs/architecture/nfr-security.md)
- [`../config/0.4-bnweb-limits.toml`](../config/0.4-bnweb-limits.toml)

## Architecture locks (do not violate)

| Lock | Implication for 0.4.6 |
| --- | --- |
| HOST policy on compiled path (GC-POL) | Policy bits and `bn_rt` re-check must be honest for claimed HOST subset |
| Fail-closed over marketing | No default-permissive behaviour on paths labeled sandboxed without an explicit “trusted HostEnv::system” doc row |
| Evidence | Each gate cites ≥1 test path and one evidence note under `docs/superpowers/evidence/` |

## Non-goals

- LLVM matrix / `examples/` promotion → **0.4.7**.
- AQ-22 `EXTERN` grammar; G6 carve-out B packaging; dynamic plugins; wasm claims.
- Closing Low items without evidence (DEFER with owner is allowed).

## Success claim (this bucket only)

Done when G0–G5 are each **Fixed** or **Deferred** (with Owner + residual risk + Defer-until), every High/Med finding has that disposition, living BN-SEC register matches code for 001–006, and the standard Rust gate is green on the security diff:

`cargo fmt --check` · `cargo test --locked --all-targets -- --test-threads=1` · `cargo clippy --locked --all-targets -- -D warnings` · `git diff --check`

---

## Locked decision — F-01 multi-A egress (before code)

**Rule I — connect-to-allowlisted-only (LOCKED 2026-09-07):**

1. Resolve DNS to the full address candidate set.
2. **Filter** candidates with the same scheme/port/SSRF/**and** egress CIDR checks used today for policy validate.
3. **Connect only** to a surviving allowlisted address (implementation may try survivors in resolver order).
4. If **no** candidate survives → **fail closed** (no connect).
5. Forbidden: validate/`connect` using only `addresses.first()` when that address was not individually allowlisted after the full check.

**Not chosen for 0.4.6:** Rule A (“reject unless *all* A/AAAA are allowlisted”) — stricter product policy; open an AQ if Carlos wants it later.

Evidence must include a multi-A fixture where the first address is out-of-CIDR and a later one is allowlisted → connect only to the allowlisted (or fail if none). Inverse: first allowlisted, later not → must not connect to the non-allowlisted.

---

## Audit inventory

| Id | Sev | Area | Disposition target | Owner |
| --- | --- | --- | --- | --- |
| F-01 | High | Egress multi-A | **Must Fixed** (Rule I) | Doug |
| F-02 | Med | POLICY_CLOCK / FILESYSTEM / RANDOM vs `bn_rt` | Fixed **or** Deferred | Doug |
| F-03 | Med | CookieJar / Set-Cookie HTTP | **Must Fixed** (BN-SEC-006) | Doug |
| F-04 | Med | HOST.Random non-CSPRNG | Fixed (doc+rails) **or** Deferred | Doug |
| F-05 | Med | FS default / rooted TOCTOU | Fixed **or** Deferred | Doug |
| F-06 | Med | DataFrame FFI harden | **Must Fixed** | Doug |
| F-07 | Low | 503 on admit failure | Optional Fixed / Deferred OK | Doug |
| F-08 | Low | Threat model + BN-SEC missing | **Must Fixed** (docs) | Doug |
| F-09 | Low | macOS keychain PEM | Fixed if cheap / Deferred | Doug |
| F-10 | Low | arp/ndp PATH | Optional / Deferred | Doug |
| R-01…R-03 | Residual | SSRF/session/lifecycle register | Close or residual text | Doug |

---

## SECTION 0 — Living security authority

- [X] **ACTIVITY 0.1** — Restore threat model + BN-SEC register under versioned docs (`docs/security/` **preferred**; else `docs/architecture/security/`), linked from `nfr-security.md`.  
  **Owner:** Doug · **Accept:** files exist; ≥1 inbound link from nfr-security; register table columns = `{id, title, status, evidence_path, owner, updated}`.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g0-register.md`
- [X] **ACTIVITY 0.2** — Reconcile BN-SEC-001…007 with current code; each row cites test path or explicit Open/Residual.  
  **Owner:** Doug · **Accept:** 7/7 rows filled; no “TBD” status.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g0-register.md`
- [X] **GATE G0** — **Owner:** Doug · **Accept:** 0.1+0.2 Fixed; zero broken links from nfr-security to register/threat model.

---

## SECTION 1 — P0 egress pin (F-01) — Rule I

- [X] **ACTIVITY 1.1** — Implement Rule I on HTTP client connect path (`src/http.rs` / `src/web.rs` as applicable).  
  **Owner:** Doug · **Accept:** code path never connects to a non-allowlisted resolved address; Rule I cited in nfr-security or BN-SEC row.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g1-f01.md` + test names
- [X] **ACTIVITY 1.2** — Multi-A regressions: (a) first out-of-CIDR / later allowlisted; (b) inverse; (c) none allowlisted → fail closed.  
  **Owner:** Doug · **Accept:** three tests green under `cargo test` filter documented in evidence.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g1-f01.md`
- [X] **ACTIVITY 1.3** — Extend/adjust egress unit tests (replace single-A-only assumptions).  
  **Owner:** Doug · **Accept:** old single-A tests still green; new multi-A tests listed in evidence.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g1-f01.md`
- [X] **GATE G1 (F-01)** — **Owner:** Doug · **Accept:** 1.1–1.3 Fixed; F-01 status Fixed in register; Rust gate green on the egress diff.

---

## SECTION 2a — Policy bits (F-02) — GATE G2a

- [X] **ACTIVITY 2a.1** — Gate `bn_rt_clock_now` / `bn_rt_clock_timer` with `POLICY_CLOCK` (mirror Console/Net).  
  **Owner:** Doug · **Accept:** denied policy → stable error/status, no clock side effect; ≥1 test.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g2a-policy.md`
- [X] **ACTIVITY 2a.2** — `POLICY_FILESYSTEM` / `POLICY_RANDOM`: **either** wire on every compiled/`bn_rt` path that exists **or** remove bits from ABI docs and register “interpret-only” with Owner+Defer-until if compile path will gain them later.  
  **Owner:** Doug · **Accept:** no ABI doc implies coverage the code lacks; grep/evidence lists each bit’s fate.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g2a-policy.md`
- [X] **GATE G2a (F-02)** — **Owner:** Doug · **Accept:** 2a.1 Fixed; 2a.2 Fixed **or** Deferred with Owner+risk+Defer-until; F-02 row updated.

---

## SECTION 2b — DataFrame FFI (F-06) — GATE G2b

- [X] **ACTIVITY 2b.1** — Bound name/`CStr` length on `bn_rt_dataframe_*`.  
  **Owner:** Doug · **Accept:** overlong/null → stable status; ≥1 hostile test; no process abort on that case.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g2b-ffi.md`
- [X] **ACTIVITY 2b.2** — Validate `BNValueKind` before union access; invalid kind → stable status.  
  **Owner:** Doug · **Accept:** ≥1 bad-kind test green.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g2b-ffi.md`
- [X] **ACTIVITY 2b.3** — `catch_unwind` (or equivalent) on dataframe FFI boundary → stable status; review `dispatch_abi` only if same pattern is in-diff.  
  **Owner:** Doug · **Accept:** panic inside provider surfaces status, not process abort, for covered entry points listed in evidence.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g2b-ffi.md`
- [X] **GATE G2b (F-06)** — **Owner:** Doug · **Accept:** 2b.1–2b.3 Fixed; F-06 Fixed in register.

---

## SECTION 3 — Cookies / session HTTP (F-03, R-02)

- [X] **ACTIVITY 3.1** — Serialize typed cookie → `Set-Cookie` from `CookieOptions` / SessionStore.  
  **Owner:** Doug · **Accept:** API path produces header string; ≥1 unit test.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g3-cookies.md`
- [X] **ACTIVITY 3.2** — Reject `Secure` cookies on cleartext responses; defaults remain Secure/HttpOnly/SameSite=Lax for TLS.  
  **Owner:** Doug · **Accept:** cleartext+Secure → error; TLS defaults documented + tested.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g3-cookies.md`
- [X] **ACTIVITY 3.3** — Tests: set / clear / rotate session id appear as headers when API used.  
  **Owner:** Doug · **Accept:** three behaviours asserted in tests named in evidence.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g3-cookies.md`
- [X] **GATE G3 (BN-SEC-006)** — **Owner:** Doug · **Accept:** 3.1–3.3 Fixed; BN-SEC-003/006 rows updated; R-02 closed or residual text.

---

## SECTION 4a — Random honesty (F-04) — GATE G4a

- [X] **ACTIVITY 4a.1** — Document `HOST.Random` as **non-cryptographic**; session/request ids remain on `ring`/CSPRNG only (cite paths).  
  **Owner:** Doug · **Accept:** nfr-security or register row states non-crypto; grep shows session ids not using HOST.Random.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g4a-random.md`
- [ ] **ACTIVITY 4a.2 (optional, not critical path)** — CSPRNG-backed API or seed policy for HOST.Random.  
  **Owner:** Doug · **Accept if done:** API+tests; **if skipped:** explicit non-goal in register (not a G4a blocker).  
  **Evidence:** optional note
- [X] **GATE G4a (F-04)** — **Owner:** Doug · **Accept:** 4a.1 Fixed; F-04 Fixed or Deferred with Owner+risk+Defer-until. **4a.2 does not block G4a.**

---

## SECTION 4b — Filesystem default + TOCTOU (F-05) — GATE G4b

- [X] **ACTIVITY 4b.1** — Lock default for untrusted HostEnv: **fail-closed / rooted** for sandbox profiles; document `HostEnv::system` as explicit trust.  
  **Owner:** Doug · **Accept:** one locked sentence in nfr-security + register; code default matches doc for sandbox constructor.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g4b-fs.md`
- [X] **ACTIVITY 4b.2** — Rooted mode: mitigate symlink escape (Linux `O_NOFOLLOW` and/or post-open revalidation; document macOS behaviour).  
  **Owner:** Doug · **Accept:** symlink-escape attempt test fails closed on CI Linux; macOS noted if divergent.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g4b-fs.md`
- [X] **ACTIVITY 4b.3 (corrective, 2026-09-09)** — Eliminate the check/use
  race left by canonicalization-only path checks. Pin rooted directories and
  perform open, create, truncate, append, delete, and log-file traversal
  relative to descriptors without following symlinks.
  **Owner:** Doug · **Accept:** deterministic root/intermediate/final symlink
  replacements and a concurrent swap test never access the outside target;
  interpreter and compiled paths share the primitive.
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g4b-fs.md`
- [X] **GATE G4b (F-05)** — **Owner:** Doug · **Accept:** 4b.1–4b.3 Fixed or Deferred with Owner+risk+Defer-until; F-05 row updated.

---

## SECTION 5 — Register closure + Low residuals

- [X] **ACTIVITY 5.1** — BN-SEC-001/002 SSRF evidence (mapped/CGNAT tests) or explicit residual.  
  **Owner:** Doug · **Accept:** each id Fixed with test path **or** Residual with risk+Defer-until.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md`
- [X] **ACTIVITY 5.2** — F-07 503-on-admit: implement **or** DEFER (Owner+risk+Defer-until).  
  **Owner:** Doug · **Accept:** Fixed with test **or** Deferred row complete.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md`
- [X] **ACTIVITY 5.3** — F-09 / F-10: cheap Fixed **or** Deferred row complete.  
  **Owner:** Doug · **Accept:** both ids have Fixed or Deferred.  
  **Evidence:** `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md`
- [X] **GATE G5** — **Owner:** Doug · **Accept:** G0 register matches code; every High/Med (F-01…F-06) is Fixed or Deferred-with-owner; Lows F-07/09/10 disposed; R-01…R-03 closed or residual.

---

## Sprint aggregation (Track A)

| Sprint | Gates | Why together | Depends on |
| --- | --- | --- | --- |
| S0 | G0 | Authority before “register updated” claims | — |
| S1 | G1 | P0 High isolable | S0 optional parallel for docs |
| S2 | G2a then G2b | Same `bn_rt` honesty theme; **separate** acceptances | S1 soft |
| S3 | G3 ∥ G4a ∥ G4b | Independent HOST surfaces | G0 for register rows |
| S4 | G5 | Close register after code dispositions | S0–S3 |
| Release | see WBS-0.4.6-release | After 0.4.6 G5 **and** 0.4.7 G7.3 | both buckets |

## Traceability

| Finding | Section / gate |
| --- | --- |
| F-08 | §0 / G0 |
| F-01 | §1 / G1 |
| F-02 | §2a / G2a |
| F-06 | §2b / G2b |
| F-03, R-02 | §3 / G3 |
| F-04 | §4a / G4a |
| F-05 | §4b / G4b |
| F-07, F-09, F-10, R-01… | §5 / G5 |
