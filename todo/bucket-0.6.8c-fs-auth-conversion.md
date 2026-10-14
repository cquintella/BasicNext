# Bucket 0.6.8c — FileSystem ops, BNWeb Auth, STRING ↔ numeric conversion (from 0.6.3 recovered)

Status: Active planning bucket (opened 2026-09-14; expanded 2026-09-19).
Depends on: 0.5.0 closed (`v0.5.0`); 0.5.1 and 0.5.2 claimed or Carlos writes an explicit deferral before tagging 0.5.3.
Gate: Carlos.
Tag policy: complete closed scope for S1+F1, FS, and A rows locked below — no half-wired SQL, filesystem, or auth. Prefer escopo / recorte / versão.
Depends also on: **0.5.2c BNCrypto** green before Auth A0 can claim crypto primitives.
Next: 0.5.4 = ORM (`ongoing/bucket-0.5.4.md`) — out of this tag.

Authoritative proposals:
- S1 / F1 — [`todo/proposals/bnsqlite-0.5.3.md`](../todo/proposals/bnsqlite-0.5.3.md)
- FS — [`todo/proposals/host-filesystem-ops-0.5.3.md`](../todo/proposals/host-filesystem-ops-0.5.3.md)
- A — [`todo/proposals/bnweb-auth-0.5.3.md`](../todo/proposals/bnweb-auth-0.5.3.md)
- Superseded — [`todo/proposals/host-sqlite-v0.md`](../todo/proposals/host-sqlite-v0.md) (`HOST.SQLite` direction A)
- General FFI beyond F1 — [`todo/proposals/c-ffi.md`](../todo/proposals/c-ffi.md) follow-on, not silent claim

Not in this bucket: ORM (0.5.4); HOST.Ui / BNUI; I1 / O2–O4 leftovers if any; full general c-ffi.

---


## Candidate — STRING ↔ numeric conversion (Carlos 2026-09-19, not yet in claim)

Need (found while rewriting `examples/objects.bn`): the language has no
way to turn an `INTEGER`/`FLOAT` into a `STRING` (no `STR`, no `AS STRING`;
`cast-suffix` is numeric/`BOOLEAN` only) nor a `STRING` into a number
(`Float.TryParse` is listed under "still outside 0.5"). Today the only
outlets are `PRINT` with commas and `BNString`. Concatenating a number into
a message (`"Alien " + id`) is a `NAME_NOT_FOUND` / `TYPE_MISMATCH`.

Ask: conversions in both directions —
- numeric → `STRING`: candidate surface `value AS STRING` (extends
  `cast-suffix`; total, never fails) or a `BNString` function;
- `STRING` → numeric: must be fallible — candidate `text AS INTEGER`
  yielding `INTEGER OR Error` (consistent with 0.5.2 O1 downcast returning
  `T OR Error`, no exceptions), with the accepted lexical form (sign, radix,
  whitespace, locale = none) fixed in the spec.

Open for lock: (1) `AS STRING` vs library function; (2) `T OR Error`
vs `T OR NULL` for the parse direction; (3) `FLOAT` formatting (`%.17g`
as `PRINT` uses today, or shortest round-trip). Needs a proposal
`todo/proposals/string-numeric-conversion-0.5.3.md` before entering the
claim table.

## Scope (locked claim — Carlos 2026-09-14 direction B; expanded 2026-09-19)

| ID | Item | Spec | Claim when done |
| --- | --- | --- | --- |
| S1 | `BNSqlite` — aparte module with native backend (not `HOST.SQLite`) | bnsqlite-0.5.3.md | Open/Close/Begin/Commit/Rollback; `Exec(sql)` → `VOID OR Error`; `Query(sql)` → `Data.DataFrame OR Error`; fail-closed; BNData integration; fixtures green |
| F1 | Minimum FFI / native module extension so S1 is honest | bnsqlite-0.5.3.md § F1 (+ c-ffi follow-on) | BNSqlite can call native SQLite without pretending pure `.bn` speaks SQL; general FFI beyond that need is explicit follow-on, not unmarked deliverable |
| FS1–FS5 | `HOST.FileSystem` ops — directories, list, copy/move, metadata/attributes (extend existing capability) | host-filesystem-ops-0.5.3.md | CreateDirectory/RemoveDirectory; List; Copy/Move; Stat (+ SetMode where portable); DeleteFile parity; fixtures green; fail-closed |
| FS6 | Working directory helpers | same | `GetWorkingDirectory` / `ChangeDirectory` if locked in; else explicit cut |
| FS7 | PickDirectory (OS dialog) | same | Optional — only if Carlos locks; else defer to HOST.Ui |
| A0–A2 | BNWeb Auth — session + OAuth; **primitives from BNCrypto (0.5.2c)** | bnweb-auth-0.5.3.md | Each locked A-row has fixtures + docs; unlocked rows cut/deferred in writing before tag |

Policy (Carlos): Host does not embed an SGBD. Other databases = aparte modules later (`BNPostgres`, …) — out of this claim. ORM = 0.5.4. File ops stay on `HOST.FileSystem`, not a new `BNFiles` module.

---

## Locks already closed

- Direction B: `IMPORT BNSqlite AS Db` (or equivalent module alias), not `IMPORT HOST.SQLite`.
- API spirit from host-sqlite-v0 (Exec vs Query split, fail-closed, BNData frames) — relocated under BNSqlite.
- F1: only the native wire necessary for BNSqlite; full c-ffi surface is not required to close 0.5.3 if delimited and listed as follow-on.
- 2026-09-19: 0.5.3 claim = SQLite + FileSystem ops + BNWeb Auth; ORM deferred to 0.5.4.
- File work extends `HOST.FileSystem` (path APIs); GUI folder picker is FS7 optional or Ui follow-on.

---

## Hard acceptance gate (no done-oco)

Do not tag 0.5.3 until:

1. Pré-req §1 satisfied.
2. API + F1 load model locked (§2).
3. Spec/proposals + book proposed then reconciled (§3, §10).
4. Fixtures-first green for Sqlite open/exec/query/close, deny policy, DataFrame (§4–§7).
5. F1 minimum wired; S1 interpret green; native parity if claimed (§5–§7).
6. Locked FS rows green (§8); locked Auth rows green (§9); cuts written for anything deferred.
7. Quorra gates + Rust evidence + release notes + Carlos tag (§10).

---

## EAP — Estrutura analítica do projeto (ordem crítica)

### 1 Pré-requisitos — superseded: 0.5.x line closed

- [x] 1.1 0.5.0 closed (`v0.5.0`).
- [ ] 1.2 0.5.1 claimed or explicit Carlos deferral.
- [ ] 1.3 0.5.2 claimed or explicit Carlos deferral.
- [ ] 1.4 Record pré-req in History when packaging starts.

DoR: tags/deferrals exist. DoD: 1.1–1.4.

### 2 Lock API BNSqlite + modelo de carregamento nativo (F1 mínimo) — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 2.1 Freeze BNSqlite surface: Open/Close/Begin/Commit/Rollback; Exec; Query; Error codes sketch; fail-closed misuse.
- [ ] 2.2 Lock import form (`IMPORT BNSqlite AS Db`) and relation to BNData (`IMPORT BNData AS Data`).
- [ ] 2.3 Delimit F1: which EXTERN / native-stdlib / load path BNSqlite needs; list what remains in general c-ffi follow-on.
- [ ] 2.4 Register locks in proposal + this History.

DoR: §1.1. DoD: no open forks inside the claim. Blocks: §3–§7.

### 3 Spec / propostas + book (proposed) — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 3.1 Keep `bnsqlite-0.5.3.md` authoritative; host-sqlite-v0 marked superseded.
- [ ] 3.2 Draft library/book sections marked proposed / not shipped.
- [ ] 3.3 Cross-link architecture (native-stdlib-binding / AQ-22) for F1 only as needed.

DoR: §2. DoD: Quorra docs skim OK; no shipped lies.

### 4 Fixtures-first — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 4.1 Open / Close round-trip.
- [ ] 4.2 Exec DDL/DML → `VOID OR Error`.
- [ ] 4.3 Query → `Data.DataFrame OR Error` (incl. empty frame).
- [ ] 4.4 Begin/Commit/Rollback sketch.
- [ ] 4.5 Fail-closed: Exec+SELECT, Query+non-row, not-open, policy deny.
- [ ] 4.6 Evidence under `docs/superpowers/evidence/bnsqlite-0.5.3/` (`program.bn` + `NOTES.md`); red OK until §6.

DoR: §2–§3 paths reserved. DoD: suites exist with commands + expected exits.

### 5 F1 — wire mínimo — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 5.1 Implement only the native binding path BNSqlite requires (per §2.3).
- [ ] 5.2 Prove a `.bn` program cannot “fake” SQL without the native backend.
- [ ] 5.3 Document follow-on FFI items not claimed.
- [ ] 5.4 Gate Quorra F1.

DoR: §4 sketched. DoD: S1 can call native SQLite honestly.

### 6 S1 — interpret — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 6.1 Frontend types + module resolution for BNSqlite.
- [ ] 6.2 Interpreter / HostEnv (or native-stdlib) path for Open/Exec/Query/Close (+ tx).
- [ ] 6.3 Fixtures §4 green on `bn run` / check as applicable.
- [ ] 6.4 Gate Quorra S1 interpret.

DoR: §5. DoD: claim rows green on interpret.

### 7 S1 — native parity (onde couber) — superseded: shipped by `done/bucket-0.6.4-bnsqlite.md`

- [ ] 7.1 Inventory LLVM/`bn_rt` needs for BNSqlite.
- [ ] 7.2 If in claim: parity fixtures; if deferred: explicit Quorra/Carlos note (no silent skip).
- [ ] 7.3 Support-matrix / ABI honesty.

DoR: §6. DoD: matrix matches claim.

### 8 FileSystem ops (FS1–FS5, optional FS6/FS7)

- [ ] 8.1 Lock List return type, Stat fields, Copy/Move overwrite, symlink policy (proposal open questions).
- [ ] 8.2 Fixtures-first: mkdir/list/stat/copy/move/delete (+ fail-closed).
- [ ] 8.3 Implement on interpret; native parity if claimed.
- [ ] 8.4 Decide FS7 PickDirectory in or cut.
- [ ] 8.5 Gate Quorra FS.

DoR: FS proposal skimmed. DoD: locked FS rows green.

### 9 BNWeb Auth (A0–A2)

- [ ] 9.1 Lock NIST PQ algorithm names, OAuth grants, session store (Sqlite vs file), BNCrypto via **0.5.2c** (must be claimed/green); OAuth/session locks.
- [ ] 9.2 Cut or defer any unlocked PQ/OAuth row in writing.
- [ ] 9.3 Fixtures for each locked A-row.
- [ ] 9.4 Implement + Quorra Auth gate.

DoR: Auth proposal locks closed enough for Ready. DoD: locked A-rows green; no crypto theater.

### 10 Docs shipped + gate + tag


- [ ] 8.1 Book/library reconciled to shipped.
- [ ] 8.2 Per-axis Quorra gates (F1, S1).
- [ ] 8.3 Rust gate paste (`fmt` / `test` / `clippy` / `git diff --check`).
- [ ] 8.4 Tag 0.5.3 after Carlos.

DoR: §5–§7. DoD: tag + notes.

---

## Explicitly not in 0.5.3 (unless Carlos expands)

- `HOST.SQLite` (direction A — superseded)
- Postgres / MySQL / other SGBD modules
- ORM / connection pools / identity map — see 0.5.4
- Prepared statements / binds (optional annotation outside S1 unless locked)
- HOST.Ui / BNUI / helpdesk GUI (PickDirectory only if FS7 locked)
- Full general-purpose c-ffi surface beyond F1 minimum
- Recursive wipe-tree delete by default; file watchers; mmap
- Pulling unfinished 0.5.1 / 0.5.2 into a partial 0.5.3 tag without deferral text

---

## Risks

| Risk | Mitigation |
| --- | --- |
| F1 balloons into full FFI | §2.3 delimitation; follow-on list; Quorra reject scope creep |
| BNData not ready for Query frames | Depend on existing DataFrame OR Error path; fixture fail-closed |
| Embedding SQLite in Host by mistake | Direction B; no HOST.SQLite registry row |
| Tagging before 0.5.1/0.5.2 | §1.2–1.3 |

---

## History

- **2026-09-19** — Carlos: **BNCrypto** moved to letter bucket **0.5.2c** (not inside 0.5.3 claim).

- 2026-09-19 — Carlos: 0.5.3 = SQLite + Arquivos + Autenticação web; 0.5.4 = ORM. Added FS proposal + Auth proposal; expanded claim table and EAP §8–§10.
- 2026-09-19 — Carlos: add candidate STRING ↔ numeric conversion (both directions); not in claim until a proposal + lock.
- 2026-09-07 — Draft `HOST.SQLite` proposal (direction A).
- 2026-09-14 — Carlos: direction B — Host does not embed SGBD; open bucket 0.5.3 with S1 `BNSqlite` + F1 minimum native wire; other DB modules later; EAP 1–8.


## Triage (2026-10-09)

- S1 + F1 (`BNSqlite`, §1–§7): shipped by `done/bucket-0.6.4-bnsqlite.md`
  (`modules/bn/BNSqlite.bn`, `crates/bn_lib_sqlite`). Those sections are
  superseded; their checkboxes are not tracked here.
- FS1–FS7 (§8): not implemented. `HOST.FileSystem` exposes `Open` and
  `DeleteFile` only; no directory, list, copy/move, stat or working-dir ops.
- A0–A2 (§9): partial. `BNWeb` has `SessionStore` and `CookieJar`; no OAuth
  and no PQ rows. TODO: confirm with Carlos whether `SessionStore` meets A0
  or was built for another claim.
- STRING ↔ numeric candidate: not in `language/0.6/` (`Float.TryParse`
  still listed as outside); `BNString.ToString` exists only for `BNString`.

Remaining work rescheduled after 0.6.7 as 0.6.8c.
