# Proposal: HOST.SQLite capability (v0) - Exec vs Query

**Status:** Proposed - design only (no IR / host / bn_rt / module stub in this document).
**Date:** 2026-09-07
**Owner (tracker):** Doug until Carlos names implementer.
**Motivation:** Give Basic Next a teachable, portable path to embedded SQL without new language keywords. SQLite is the first concrete database capability: file-backed, ubiquitous, and a natural producer/consumer of tabular data already modeled by **BNData.DataFrame**.

**API evolution in this draft:** Carlos first asked for one Exec with multiple success shapes, then for a RESULT+DF struct, then (2026-09-07) preferred **splitting SELECT from the rest** because **only SELECT returns a DataFrame**. This revision locks that split: clear return types, no triple alternative, no result wrapper class.

**Related (do not conflate):**
- [PHILOSOPHY.md](../../PHILOSOPHY.md) - small core, HOST capabilities, explicit contracts, KISS
- [host-capabilities.md](host-capabilities.md) - exploratory HOST pattern
- [host-ui-bnui-v0.md](host-ui-bnui-v0.md) - sibling capability slice (UI)
- [docs/library/bndata.md](../../docs/library/bndata.md) - Data.DataFrame / DataFrame OR Error patterns

Nothing here is normative until accepted into docs/language/ / docs/library/host.md (or successor) and fixtures.

---

## Problem

| Gap | Today |
| --- | --- |
| Persistence | FileSystem + CSV via BNData; no SQL HOST contract |
| Teaching | Need SQL without ORM or new keywords |
| Typing | Mixing "maybe a frame" into one Exec forces DataFrame OR VOID OR Error or a RESULT/DF struct - heavier than two methods |
| Errors | Open and SQL must return OR Error; caller captures |

---

## Goals (v0)

1. Capability **HOST.SQLite** (`IMPORT HOST.SQLite AS Db`) - no new keywords.
2. Split SQL by return shape:
   - **`Exec(sql)`** - statements **without** a row result set -> `VOID OR Error`
   - **`Query(sql)`** - row-producing statements (SELECT and host-allowed equivalents) -> `Data.DataFrame OR Error`
3. **Open** / **Close** / **Begin** / **Commit** / **Rollback** as connection/session control (`VOID OR Error` where fallible).
4. **Caller captures** returns with LET - no last-result register on Db.
5. Integrate with **BNData** (IMPORT BNData AS Data when binding frames).
6. Fail closed under HOST policy.

```basic
IMPORT HOST.SQLite AS Db
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

## Non-goals (v0)

- Prepared statements / binds (stringly SQL; document injection risk; binds = v0.1).
- ExecResult / RESULT+DF wrapper (rejected in favor of Exec vs Query split).
- Triple return `DataFrame OR VOID OR Error` on one method (superseded).
- Recordset/Cursor parallel to DataFrame.
- ORM, pools, network SQL drivers, BLOB, streaming cursors.
- Silent success when the author calls Query with non-SELECT (fail closed -> Error) or Exec with SELECT (fail closed -> Error directing to Query).

---

## Design principles

- **KISS / explicit types:** each method has one success arm (VOID or DataFrame), plus Error.
- **Capability, not keyword.**
- **One tabular type:** DataFrame only from Query.
- **Caller owns values** via LET + IS Error.
- Prefer two honest names over one overloaded Exec.

---

## Naming and methods

| Piece | Name | Notes |
| --- | --- | --- |
| Capability | HOST.SQLite | Lock registry spelling |
| Import | AS Db | Teaching default |
| Frames | Data.DataFrame | Requires IMPORT BNData when used |

| Method | Signature (illustrative) | Role |
| --- | --- | --- |
| Open | Open(path AS STRING) AS VOID OR Error | Open/create per flags |
| Close | Close() AS VOID OR Error | Release; prefer idempotent |
| Exec | Exec(sql AS STRING) AS VOID OR Error | DDL/DML / non-row SQL |
| Query | Query(sql AS STRING) AS Data.DataFrame OR Error | SELECT (row-producing only) |
| Begin | Begin() AS VOID OR Error | Transaction start |
| Commit | Commit() AS VOID OR Error | Commit |
| Rollback | Rollback() AS VOID OR Error | Rollback |

Alias **Select** for Query is optional sugar later; v0 normative name is **Query** (pairs with Exec; avoids clashing with DataFrame.Select).

### Misuse rules (fail closed)

| Call | Host behavior |
| --- | --- |
| Exec with row-producing SQL | Error (use Query) |
| Query with non-row SQL | Error (use Exec) |
| Either when not open | Error |
| Query success, zero rows | Empty DataFrame (columns from metadata when possible), not Error |

---

## Who captures the return?

The **caller**:

- `LET x AS VOID OR Error = Db.Exec(...)`
- `LET t AS Data.DataFrame OR Error = Db.Query(...)`
- Discriminate with IS Error before using the success arm.
- No RESULT/DF struct; no implicit last result on Db.

---

## Error model

| Operation | Success | Failure examples |
| --- | --- | --- |
| Open | VOID | missing file, access denied, corrupt DB, policy deny |
| Exec | VOID | syntax, constraint, not open, SELECT-used-as-Exec |
| Query | DataFrame | syntax, not open, non-SELECT, mapping, result too large |
| Begin/Commit/Rollback/Close | VOID | tx/state/I/O errors |

---

## DataFrame mapping (v0)

Same intent as CSV path: INTEGER/REAL/TEXT -> INTEGER/FLOAT/STRING; NUMERIC host-consistent; BLOB out of v0 (Error); NULL -> BNData NA rules where possible.

---

## Teaching sketch

```basic
IMPORT HOST.SQLite AS Db
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET err AS VOID OR Error = Db.Open("hello.db")
  IF err IS Error THEN
    PRINT err.Code
    RETURN
  END IF

  err = Db.Exec("CREATE TABLE IF NOT EXISTS people (id INTEGER, name TEXT)")
  IF err IS Error THEN
    PRINT err.Code
    Db.Close()
    RETURN
  END IF

  err = Db.Exec("INSERT INTO people (id, name) VALUES (1, 'Ana')")
  IF err IS Error THEN
    PRINT err.Code
    Db.Close()
    RETURN
  END IF

  LET people AS Data.DataFrame OR Error = Db.Query("SELECT id, name FROM people ORDER BY id")
  IF people IS Error THEN
    PRINT people.Code
  ELSE
    PRINT people.RowCount()
  END IF

  Db.Close()
END FUNCTION
```

---

## Why not the other shapes

| Shape | Why not for v0 |
| --- | --- |
| Exec -> DataFrame OR VOID OR Error | One method, three arms; harder to teach than Exec vs Query |
| ExecResult with RESULT + DF | Extra type; DF often empty; Error still needed as OR Error for honesty |
| Exec only, ignore frames | Cannot integrate with BNData |

---

## Implementation boundaries (when accepted)

| Layer | Responsibility |
| --- | --- |
| Spec / host docs | Capability, Exec vs Query, mapping, misuse Errors |
| Frontend | Typecheck signatures; IMPORT HOST.SQLite / BNData |
| bn_rt / host | Embed SQLite; policy; materialize DataFrame on Query |
| Support matrix | Native first |

Production bar: no stub Open/Exec/Query.

---

## Evolution (after v0)

1. Prepared statements / binds.
2. Connection object if multi-DB.
3. Optional Select alias; InsertFrame helpers.
4. BLOB support with DataFrame story locked.

---

## Open questions

1. Open flags / create-if-missing.
2. Methods on Db vs Open returns Connection.
3. Result size limit on Query.
4. BNData import mandatory vs re-export (prefer explicit IMPORT BNData).
5. Autocommit defaults for teaching.
6. Registry spelling HOST.SQLite vs HOST.Sqlite.
7. Exact Error when Exec gets SELECT / Query gets INSERT (stable codes).

---

## Decision summary

| Topic | v0 decision |
| --- | --- |
| Keywords | None new |
| Capability | HOST.SQLite |
| Non-row SQL | Exec -> VOID OR Error |
| Row SQL | Query -> Data.DataFrame OR Error |
| Who captures | Caller via LET |
| Struct RESULT+DF | Not used |
| Triple OR on Exec | Not used |
| Recordset | None - BNData only |
