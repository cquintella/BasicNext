# Proposal: HOST.SQLite capability (v0) - Exec -> DataFrame OR VOID OR Error

**Status:** Proposed - design only (no IR / host / bn_rt / module stub in this document).
**Date:** 2026-09-07
**Owner (tracker):** Doug until Carlos names implementer.
**Motivation:** Give Basic Next a teachable, portable path to embedded SQL without new language keywords. SQLite is the first concrete database capability: file-backed, ubiquitous, and a natural producer/consumer of tabular data already modeled by **BNData.DataFrame**. Carlos (2026-09-07): one **Exec(sql)** whose result is an alternative type - **DataFrame**, **VOID** (ok without rows), or **Error** - so the author does not guess statement kind up front between Query vs Exec.

**Related (do not conflate):**
- [PHILOSOPHY.md](../../PHILOSOPHY.md) - small core, HOST capabilities, explicit contracts, KISS, anti-framework
- [host-capabilities.md](host-capabilities.md) - exploratory HOST pattern
- [host-ui-bnui-v0.md](host-ui-bnui-v0.md) - sibling capability slice (UI); SQLite is independent
- [docs/library/bndata.md](../../docs/library/bndata.md) - normative Data.DataFrame / DataFrame OR Error patterns
- Architecture host traits / execution policy - opening a DB file is an effect; deny != unimplemented

Nothing here is normative until accepted into docs/language/ / docs/library/host.md (or successor) and fixtures.

---

## Problem

| Gap | Today |
| --- | --- |
| Persistence | FileSystem + CSV via BNData; no SQL engine in-tree as a HOST contract |
| Teaching | Students know table in / table out; CSV alone hides transactions and predicates |
| Typing | A parallel Recordset type would fork the tabular story owned by DataFrame |
| Errors | Open and SQL execution must be able to fail without panicking |
| Result kinds | One SQL string may yield rows, yield no rows, or fail - the API must say so in the type |

---

## Goals (v0)

1. Capability **HOST.SQLite**, imported explicitly (e.g. `IMPORT HOST.SQLite AS Db`) - no new keywords.
2. Connection-oriented API: **Open**, **Close**, **Exec**, **Begin** / **Commit** / **Rollback**.
3. Primary SQL entry:

```basic
Db.Exec(sql AS STRING) AS Data.DataFrame OR VOID OR Error
```

   - **Error** - not open, syntax, constraint, policy deny, mapping failure, ...
   - **VOID** - success with no result set (typical CREATE / INSERT / UPDATE / DELETE)
   - **Data.DataFrame** - success with a result set (typical SELECT); column names from metadata

4. **Open** returns `VOID OR Error` (or Connection OR Error if Open constructs a handle - see Open questions).
5. **Caller captures returns** with ordinary BN bindings - no last-result register on Db:

```basic
IMPORT HOST.SQLite AS Db
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET opened AS VOID OR Error = Db.Open("demo.db")
  IF opened IS Error THEN
    PRINT opened.Code
    RETURN
  END IF

  LET result AS Data.DataFrame OR VOID OR Error = Db.Exec("SELECT id, name FROM people")
  IF result IS Error THEN
    PRINT result.Code
  ELSE IF result IS VOID THEN
    PRINT "ok, sem frame"
  ELSE
    PRINT result.RowCount()
  END IF

  Db.Close()
END FUNCTION
```

6. Fail closed under HOST policy. Interpret is reference where supported; compile only with support-matrix evidence later.

## Non-goals (v0)

- Prepared statements / bound parameters (stringly SQL in v0; document injection risk; binds = v0.1).
- Connection pools, network drivers, ORM, migrations framework.
- A parallel Recordset / Cursor type competing with DataFrame.
- Streaming cursors (v0 materializes full results into a DataFrame; huge results may Error - see Open).
- Requiring a separate Query method (optional alias later).
- Claiming demos before the capability is hosted for real.

---

## Design principles

- Capability, not keyword.
- One tabular type: BNData.DataFrame.
- Explicit errors and explicit success shapes (DataFrame vs VOID).
- Caller owns the value via LET.
- KISS: one Exec name for SQL text.

---

## Naming and methods

| Piece | Name | Notes |
| --- | --- | --- |
| Capability | HOST.SQLite | Carlos spelling; lock registry normalization |
| Import alias | AS Db | Teaching default |
| Tabular result | Data.DataFrame | Programs that bind frames also IMPORT BNData AS Data |
| Optional module | none in v0 | Capability methods are enough |

| Method | Signature (illustrative) | Role |
| --- | --- | --- |
| Open | Open(path AS STRING) AS VOID OR Error | Open/create per documented flags |
| Close | Close() AS VOID OR Error | Release handle; prefer idempotent Close |
| Exec | Exec(sql AS STRING) AS Data.DataFrame OR VOID OR Error | Primary SQL entry |
| Begin | Begin() AS VOID OR Error | Transaction start |
| Commit | Commit() AS VOID OR Error | Transaction end |
| Rollback | Rollback() AS VOID OR Error | Abort transaction |

### Exec result discrimination

| Outcome | Type arm | When |
| --- | --- | --- |
| Failure | Error | SQL/host/policy/mapping failures |
| Success, no result set | VOID | DDL/DML without rows to return |
| Success, with rows | Data.DataFrame | SELECT (and other row-producing statements); zero rows => empty DataFrame (preferred), not VOID |

Teaching order: check `IS Error`, then `IS VOID`, else use as DataFrame.

---

## Who captures the return?

The **caller**:

1. Exec returns `Data.DataFrame OR VOID OR Error`.
2. Assign with LET into that alternative type.
3. Discriminate before use.
4. No implicit last Exec result on Db.

Open never returns a DataFrame - only `VOID OR Error`.

---

## Error model

| Operation | Success arms | Failure examples |
| --- | --- | --- |
| Open | VOID | missing file, access denied, corrupt DB, policy deny, already open |
| Exec | DataFrame or VOID | syntax, constraint, not open, policy, mapping, result too large |
| Begin / Commit / Rollback | VOID | no transaction, not open, I/O error |
| Close | VOID | prefer success if already closed |

---

## DataFrame mapping (v0)

- Each SQL result column -> one DataFrame column.
- Provisional mapping: INTEGER->INTEGER, REAL->FLOAT, TEXT->STRING; NUMERIC host-consistent; BLOB out of v0 (Error); NULL cells follow BNData NA rules where possible.
- Empty SELECT: empty DataFrame with columns when metadata allows (not VOID).

---

## Teaching sketch

```basic
IMPORT HOST.SQLite AS Db
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET opened AS VOID OR Error = Db.Open("hello.db")
  IF opened IS Error THEN
    PRINT opened.Code
    RETURN
  END IF

  LET r AS Data.DataFrame OR VOID OR Error = Db.Exec(
    "CREATE TABLE IF NOT EXISTS people (id INTEGER, name TEXT)"
  )
  IF r IS Error THEN
    PRINT r.Code
    Db.Close()
    RETURN
  END IF

  r = Db.Exec("INSERT INTO people (id, name) VALUES (1, 'Ana')")
  IF r IS Error THEN
    PRINT r.Code
    Db.Close()
    RETURN
  END IF

  r = Db.Exec("SELECT id, name FROM people ORDER BY id")
  IF r IS Error THEN
    PRINT r.Code
  ELSE IF r IS VOID THEN
    PRINT "SELECT sem frame - nao esperado"
  ELSE
    PRINT r.RowCount()
  END IF

  Db.Close()
END FUNCTION
```

---

## Implementation boundaries (when accepted - not this document)

| Layer | Responsibility |
| --- | --- |
| Spec / host library docs | Capability surface, alternative return, mapping, non-goals |
| Frontend | Typecheck imports and Data.DataFrame OR VOID OR Error returns |
| IR | HOST calls unless a later native-binding note says otherwise |
| bn_rt / host | Embed/link SQLite; policy on paths; build DataFrame via existing dataframe ABI |
| Support matrix | Native first; other targets unsupported until proposed |

Production bar: no stub Open/Exec that always returns VOID.

---

## Evolution (after v0)

1. Prepared statements / binds.
2. Explicit Connection object if multi-DB needs it.
3. Optional Query alias that Errors on VOID.
4. BLOB / InsertFrame helpers.
5. Optional BNSQLite sugar module.

---

## Open questions

1. Open flags: create-if-missing vs modes.
2. Methods on Db after Open vs Open returns Connection.
3. Result size limit.
4. Must programs IMPORT BNData explicitly (preferred) vs re-export.
5. Transaction autocommit defaults for teaching.
6. Registry spelling HOST.SQLite vs HOST.Sqlite.
7. Frontend ergonomics of Data.DataFrame OR VOID OR Error (IS VOID / IS Error) without a Result wrapper class.
8. Confirm empty SELECT => empty DataFrame (proposal default) vs VOID.

---

## Decision summary

| Topic | v0 decision |
| --- | --- |
| Keywords | None new |
| Capability | HOST.SQLite (IMPORT ... AS Db) |
| SQL API | Exec(sql) AS Data.DataFrame OR VOID OR Error |
| Who captures | Caller via LET |
| Separate Query | Not required |
| Recordset type | None - use BNData |
| Prepared SQL | Out of v0 |
