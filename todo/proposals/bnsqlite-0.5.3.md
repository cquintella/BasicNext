# Proposal: `BNSqlite` module with native backend — S1 / F1 (0.5.3)

**Status:** Proposed — library + native-module contract (**S1**, **F1**).
**Date:** 2026-09-14
**Target release:** **0.5.3** (after 0.5.0; after 0.5.1 and 0.5.2 claimed or explicitly deferred).
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14, direction B):** Teachable embedded SQL via an **aparte module** with a **native** SQLite backend. The Host does **not** embed an SGBD. Other databases arrive later as separate modules (`BNPostgres`, …) — out of this claim.

**Supersedes:** [`host-sqlite-v0.md`](host-sqlite-v0.md) (`HOST.SQLite` / direction A). API *spirit* (Exec vs Query, fail-closed, BNData) is preserved; the **capability home** is not HOST.

**Related:** [`c-ffi.md`](c-ffi.md) / AQ-22 / [native-stdlib-binding.md](../../docs/architecture/native-stdlib-binding.md) — F1 consumes only the **minimum** native declaration/load path needed for BNSqlite; full FFI is follow-on.
**Related data:** [docs/library/bndata.md](../../docs/library/bndata.md) — `Data.DataFrame OR Error`.

Nothing here is normative until accepted into library docs, fixtures, and (for F1) the native binding story.

---

## Intent locks (Carlos)

| ID | Lock |
| --- | --- |
| **S1** | Module **`BNSqlite`** (native backend), not `HOST.SQLite` |
| **F1** | Minimum FFI / native-module wire so S1 is honest — pure `.bn` must not pretend to speak SQL |
| Policy | Host does not embed SGBD; other DB modules are later claims |

---

## Import and teaching sketch

```basic
IMPORT BNSqlite AS Db
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET opened AS VOID OR Error = Db.Open("demo.db")
  IF opened IS Error THEN
    PRINT opened.Code
    RETURN
  END IF

  LET wrote AS VOID OR Error = Db.Exec("INSERT INTO people (id, name) VALUES (1, 'Ana')")
  IF wrote IS Error THEN
    PRINT wrote.Code
    Db.Close()
    RETURN
  END IF

  LET people AS Data.DataFrame OR Error = Db.Query("SELECT id, name FROM people")
  IF people IS Error THEN
    PRINT people.Code
  ELSE
    PRINT people.RowCount()
  END IF

  Db.Close()
END FUNCTION
```

Exact class vs module-function shape (e.g. connection object vs module-level session) is locked in bucket §2.1 — default sketch: module-level session matching the old HOST.SQLite v0 surface unless Carlos prefers an explicit connection type.

---

## S1 — API (spirit of host-sqlite-v0)

| Method | Signature (illustrative) | Success | Failure |
| --- | --- | --- | --- |
| Open | `Open(path AS STRING) AS VOID OR Error` | VOID | I/O, corrupt, policy deny |
| Close | `Close() AS VOID OR Error` | VOID | state / I/O |
| Exec | `Exec(sql AS STRING) AS VOID OR Error` | VOID | SQL, not open, row-SQL misuse |
| Query | `Query(sql AS STRING) AS Data.DataFrame OR Error` | DataFrame | SQL, not open, non-row misuse, map/size |
| Begin | `Begin() AS VOID OR Error` | VOID | tx / state |
| Commit | `Commit() AS VOID OR Error` | VOID | tx / state |
| Rollback | `Rollback() AS VOID OR Error` | VOID | tx / state |

### Fail-closed misuse

| Call | Behavior |
| --- | --- |
| Exec with row-producing SQL | Error (use Query) |
| Query with non-row SQL | Error (use Exec) |
| Either when not open | Error |
| Query success, zero rows | Empty DataFrame (columns from metadata when possible), not Error |

Caller captures with `LET … OR Error` and `IS Error`. No last-result register. No RESULT+DF wrapper. No triple `DataFrame OR VOID OR Error`.

### DataFrame mapping (v0)

INTEGER/REAL/TEXT → INTEGER/FLOAT/STRING; NUMERIC host-consistent; BLOB out of v0 → Error; NULL → BNData NA rules where possible.

---

## F1 — minimum native wire (not full c-ffi)

**Problem:** SQLite is a native C library. Shipping only a `.bn` facade without a real backend is done-oco.

**In claim (minimum):**

1. A documented path for BNSqlite to bind native symbols (stdlib-native profile and/or EXTERN slice per AQ-22 — **only what BNSqlite needs**).
2. Runtime / `bn_rt` (or equivalent) ownership for connection handle, Exec, Query→DataFrame materialization.
3. Evidence that interpret (and native if claimed) actually talks to SQLite.

**Explicit follow-on (not 0.5.3 silent claim):**

- General `IMPORT HOST.c` / arbitrary EXTERN libraries
- Dynamic loading of arbitrary `.so`
- Full grammar/ownership matrix beyond the BNSqlite symbols

Bucket §2.3 must list the F1 symbol set and the follow-on remainder before implementation starts.

---

## Non-goals (0.5.3)

- `HOST.SQLite`
- Postgres / MySQL / …
- ORM, pools
- Prepared / binds (annotate as possible **v0.1** outside claim)
- HOST.Ui / helpdesk app
- Full c-ffi beyond F1

---

## Acceptance fixtures (sketch)

| ID | Intent |
| --- | --- |
| SQ01 | Open + Close |
| SQ02 | Exec CREATE/INSERT |
| SQ03 | Query → DataFrame row count / values |
| SQ04 | Empty Query → empty frame |
| SQ05 | Fail-closed Exec+SELECT / Query+DDL |
| SQ06 | Policy deny (if applicable) → Error, no file side effect |
| SQ07 | F1 honesty: without native backend, claim fails closed (not a fake success) |

Evidence: `docs/superpowers/evidence/bnsqlite-0.5.3/`.

---

## History

- **2026-09-07** — API spirit drafted under `HOST.SQLite` (direction A).
- **2026-09-14** — Carlos direction **B**: relocate to **`BNSqlite`** + **F1** minimum; Host does not embed SGBD; bucket 0.5.3.

**Retarget (2026-09-21, invert):** claim lives in `todo/bucket-0.6.3.md` (was 0.5.3).
