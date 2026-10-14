# Specification (proposed): Basic Next package ecosystem

**Status:** Formal draft for CLU review — **not** normative until approved and filed.
**Date:** 2026-09-20
**Author (draft):** Quorra (coordination) for Carlos Quintella
**Audience:** CLU (infra / DNS / deploy); persist on approval at **mail.intelliurb.com** (CLU owns the save).
**Scope name:** Basic Next package ecosystem — `bnp` CLI, `bn-registry`, token auth, `.bnp` format, Warehouse layout.

---

## 1. Intent

Give Basic Next a **closed, auditable** way to distribute reusable modules and native-backed libraries **without** inventing a kitchen-sink package manager or silent network installs. Packages are first-class artifacts; the language remains capability- and import-driven (`IMPORT … AS …`).

**Non-goals (this draft):** automatic transitive resolution from the public internet without policy; npm-style scripts; language keywords for packages; replacing `modules/bn/` stdlib layout in 0.5.x without a version bucket.

---

## 2. Components

| Component | Role |
| --- | --- |
| **`bnp` CLI** | Build, pack, verify, publish (to registry), fetch (policy-gated) package artifacts |
| **`bn-registry`** | Service that indexes package metadata and serves approved `.bnp` blobs |
| **Token auth** | Machine/human credentials for publish and private fetch |
| **`.bnp` format** | On-disk / transfer archive of a package |
| **Warehouse** | Canonical on-server layout for approved specs and published package storage (InteliOrb / Basic Next) |

---

## 3. `.bnp` format (proposed)

- **Container:** zip or tar.gz (lock one; recommendation: **zip** with deterministic entry order for reproducible hashes).
- **Root manifest:** `bnp.toml` (or `bnp.json`) with:
  - `name` (dotted id, e.g. `org.inteliorb.demo`)
  - `version` (semver)
  - `bn_min` / `bn_max` (language line compatibility)
  - `exports` (list of BN modules paths inside the archive)
  - `native` (optional: list of platform keys → staticlib / dylib names)
  - `digest` algorithm: **SHA-256** of payload excluding the digest field itself
- **Content:** `.bn` sources under `modules/`; optional `native/<platform>/`; optional `docs/`, `LICENSE`.
- **Integrity:** outer SHA-256 published by registry; optional Ed25519 signature of the digest (aligns with BNCrypto when available).

---

## 4. `bnp` CLI (proposed commands)

| Command | Behaviour |
| --- | --- |
| `bnp init` | Scaffold manifest + module layout |
| `bnp pack` | Produce `.bnp` from a directory; write digest |
| `bnp verify` | Check digest / signature |
| `bnp publish` | Authenticated upload to `bn-registry` |
| `bnp fetch <name>@<version>` | Download into a **local** cache; does not mutate system stdlib without explicit install |
| `bnp install` | Materialize into a project `modules/` or `BN_HOME` overlay (policy) |

Flags: `--registry URL`, `--token` / token file, `--dry-run`. Fail-closed on hash mismatch.

---

## 5. `bn-registry` (proposed)

- HTTP API (HTTPS only in production):
  - `GET /v1/packages/{name}` → metadata
  - `GET /v1/packages/{name}/{version}/bnp` → artifact
  - `PUT /v1/packages/{name}/{version}` → publish (auth)
- Indexes: name, version, digests, yank flag, bn compatibility.
- Storage backend: object store or filesystem under Warehouse (see §7).
- No anonymous publish. Fetch may be public for open packages; private packages require token.

---

## 6. Token auth (proposed)

- **Bearer tokens** issued by registry (opaque, high entropy); stored hashed server-side (Argon2id or HMAC of random when BNCrypto lands).
- Scopes: `publish`, `fetch-private`, `admin`.
- Transport: `Authorization: Bearer …` or `BNP_TOKEN` env for CLI.
- Rotation / revoke endpoints. No passwords in CLI argv history.
- Future: OAuth / OIDC for humans; machine tokens remain primary for CI.

---

## 7. Warehouse layout (proposed)

Canonical InteliOrb layout for Basic Next package + approved specs:

```text
InteliOrb/
  Basic Next/
    Warehouse/
      specs/           # approved specifications (markdown/PDF)
      packages/        # registry blob store: {name}/{version}/*.bnp
      indexes/         # registry metadata mirrors / dumps
      tokens/          # server-side token material (not in git; ACL-restricted)
```

**Delivery rule (2026-09-20 voice lock):** after **CLU** reviews and **approves** this specification, **CLU** persists the approved file on the server associated with **mail.intelliurb.com** (CLU-owned save — Quorra does not email the blob as the primary delivery).

---

## 8. Relation to existing Basic Next toolchain

- Stdlib remains `share/bn/modules/bn` / `BN_HOME` discovery.
- Native runtime `libbn_rt.a` stays under `$PREFIX/lib` (install scripts); packages may ship **additional** native libs declared in `.bnp`, not replace `bn_rt` silently.
- Language still has **no** package registry in 0.5 claim tables until a versioned bucket accepts this spec.

---

## 9. Open questions for CLU

1. Exact host/path on **mail.intelliurb.com** for `Warehouse/specs/` (absolute path).
2. Registry hostname (e.g. `registry.intelliurb.com` vs path under mail host).
3. Zip vs tar.gz for `.bnp`.
4. Whether publish requires human 2FA in addition to token.
5. Backup / retention policy for `packages/`.

---

## 10. Acceptance for this review

CLU marks **APPROVE** or **REJECT** (with reasons). On **APPROVE**, CLU saves this document under Warehouse specs on mail.intelliurb.com and returns the **absolute saved path**. Quorra reports the verdict and path to Carlos.
