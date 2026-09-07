# Proposal: HOST.SQLite capability (v0) — Query → DataFrame

**Status:** Proposed — design only (no IR / host / `bn_rt` / module stub in this document).  
**Date:** 2026-09-07  
**Owner (tracker):** Doug until Carlos names implementer.  
**Motivation:** Give Basic Next a teachable, portable path to embedded SQL without new language keywords. SQLite is the first concrete database capability: file-backed, ubiquitous, and a natural producer/consumer of tabular data already modeled by **`BNData.DataFrame`**.

**Related (do not conflate):**
- [PHILOSOPHY.md](../../PHILOSOPHY.md) — small core, HOST capabilities, explicit contracts, KISS, anti-framework
- [host-capabilities.md](host-capabilities.md) — exploratory HOST pattern
- [host-ui-bnui-v0.md](host-ui-bnui-v0.md) — sibling capability+module slice (UI); SQLite is independent
- [docs/library/bndata.md](../../docs/library/bndata.md) — normative `Data.DataFrame` / `DataFrame OR Error` patterns (`ReadCSV`, joins, …)
- Architecture host traits / execution policy — opening a DB file is an **effect**; deny ≠ unimplemented

Nothing here is normative until accepted into `docs/language/` / `docs/library/host.md` (or successor) and fixtures.

---

## Problem

| Gap | Today |
| --- | --- |
| Persistence | FileSystem + CSV via BNData; no SQL engine in-tree as a HOST contract |
| Teaching | Students know “table in / table out”; forcing only CSV hides transactions and predicates |
| Typing | Ad-hoc “recordset” types would fork the tabular story already owned by `DataFrame` |
| Errors | Open and query must be able to fail without panicking the process |

Carlos: integrate with DataFrame; the **caller** captures query results; **Open** and **Query** (and other fallible ops) return **`OR Error`**.

---

## Goals (v0)

1. Capability **`HOST.SQLite`**, imported explicitly (e.g. `IMPORT HOST.SQLite AS Db`) — **no** new keywords (`SQL`, `DATABASE`, …).
2. Connection-oriented API on the imported capability object (or a connection object it returns — see Open questions): **Open**, **Close**, **Exec**, **Query**, **Begin** / **Commit** / **Rollback**.
3. **`Query` returns `Data.DataFrame OR Error`**. Column names come from the result-set metadata (or generated `column_N` when unnamed). Cell types map into DataFrame column kinds already supported (`STRING` / `INTEGER` / `FLOAT` / `BOOLEAN` / NA rules as for CSV where applicable).
4. **`Open` returns `VOID OR Error`** (or `Connection OR Error` if Open constructs a handle — see below). Failure modes include missing file (when not create), permission, corrupt DB, policy deny.
5. **Caller captures returns** with ordinary BN bindings — no hidden globals, no “last result” register:

```basic
IMPORT HOST.SQLite AS Db
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET opened AS VOID OR Error = Db.Open("demo.db")
  IF opened IS Error THEN
    PRINT opened.Code
    RETURN
  END IF

  LET table AS Data.DataFrame OR Error = Db.Query("SELECT id, name FROM people")
  IF table IS Error THEN
    PRINT table.Code
  ELSE
    PRINT table.RowCount()
  END IF

  Db.Close()
END FUNCTION
```

6. Fail closed under HOST policy (path allowlists / capability deny). Interpret is reference where supported; compile only with support-matrix evidence later.

## Non-goals (v0)

- Prepared statements / bound parameters (stringly SQL only in v0; **SQL injection is the author’s responsibility** — document loudly; binds = v0.1).
- Connection pools, network Postgres/MySQL drivers, ORM, migrations framework.
- A parallel `Recordset` / `Cursor` type competing with `DataFrame`.
- Streaming cursors / incremental fetch (v0 materializes the full result into a DataFrame; huge results may Error or be host-limited — see Open).
- Multi-connection fancy graphs; v0 assumes one active connection per imported alias unless Open returns an explicit connection object.
- Claiming UI+SQLite demos before both capabilities are accepted and hosted.

---

## Design principles

- **Capability, not keyword** — richness in `HOST.SQLite`, not language DNA.
- **One tabular type** — Query integrates with **BNData**, matching `ReadCSV`’s `DataFrame OR Error` habit so teaching stays one mental model.
- **Explicit errors** — Open, Query, Exec, Commit paths use `OR Error`; success is not assumed.
- **Caller owns the value** — `LET x AS Data.DataFrame OR Error = Db.Query(...)`; nothing captures “for” the programmer.
- **KISS** — few methods; sugar like `Insert`/`CreateTable` is optional thin wrappers over `Exec`, not a second API surface in v0 docs.

---

## Naming

| Piece | Name | Notes |
| --- | --- | --- |
| Capability | `HOST.SQLite` | Carlos’s spelling; registry must normalize consistently with other `HOST.*` names |
| Import alias | `AS Db` | Teaching default; not mandatory |
| Tabular result | `Data.DataFrame` | Requires `IMPORT BNData AS Data` in programs that bind Query results |
| Optional module | none in v0 | Unlike BNUI, v0 can be **capability-only** methods; a thin `BNSQLite` sugar module is evolution if examples need it |

Method names (preferred consistent style — all on the connection / capability):

| Method | Signature (illustrative) | Role |
| --- | --- | --- |
| `Open` | `Open(path AS STRING) AS VOID OR Error` | Open/create per documented flags (see Open questions) |
| `Close` | `Close() AS VOID OR Error` | Release handle; idempotent Close is preferred |
| `Exec` | `Exec(sql AS STRING) AS VOID OR Error` | DDL/DML without result set (`CREATE`, `INSERT`, `UPDATE`, `DELETE`, …) |
| `Query` | `Query(sql AS STRING) AS Data.DataFrame OR Error` | `SELECT` (and other row-producing statements the host allows) |
| `Begin` | `Begin() AS VOID OR Error` | Transaction start |
| `Commit` | `Commit() AS VOID OR Error` | Transaction end |
| `Rollback` | `Rollback() AS VOID OR Error` | Abort transaction |

**Not** a mix of `Db.Select` / `DbInsert` freestanding names — keep one dotted style on `Db` (or on a `Connection` object).

Optional sugar (non-normative examples only, not required in v0):

```basic
' Equivalent to Exec("CREATE TABLE ...")
Db.Exec("CREATE TABLE people (id INTEGER, name TEXT)")
```

---

## Who captures the return?

**The caller**, with a typed binding:

1. `Query` **returns** a value of type `Data.DataFrame OR Error`.
2. The program **assigns** it (`LET` / `=` into a declared `Data.DataFrame OR Error`).
3. Discriminate with `IS Error` (or equivalent accepted Error handling) before using DataFrame methods.
4. There is **no** implicit “last query result” on `Db`, no print-side capture, and no event-handler magic.

Same pattern for Open:

```basic
LET opened AS VOID OR Error = Db.Open(path)
IF opened IS Error THEN
  ' handle — do not call Query
END IF
```

This matches BNData’s CSV story and PHILOSOPHY explicit contracts.

---

## Error model

| Operation | Success | Failure examples (non-exhaustive) |
| --- | --- | --- |
| `Open` | `VOID` (or Connection) | file not found, access denied, not a DB, policy deny, already open |
| `Query` | `Data.DataFrame` | syntax error, no such table, wrong column types for mapping, not open, policy deny, result too large |
| `Exec` | `VOID` | constraint violation, syntax error, not open |
| `Commit` / `Rollback` / `Begin` | `VOID` | no transaction, not open, I/O error |
| `Close` | `VOID` | prefer success if already closed; else I/O error |

Error codes should be stable strings/codes consistent with other HOST modules once diagnostics catalog rules apply — exact code list is an implementation acceptance item, not invented here as fake DiagIds.

---

## DataFrame mapping (v0)

- Each SQL result column → one DataFrame column; names from SQLite column names when present.
- Type mapping (provisional — lock in fixtures when implementing):

| SQLite affinity / declared | DataFrame column kind (v0 intent) |
| --- | --- |
| INTEGER | `INTEGER` |
| REAL | `FLOAT` |
| TEXT | `STRING` |
| NUMERIC | host chooses INTEGER or FLOAT consistently; document in fixtures |
| BLOB | **out of v0** → Error (or STRING hex later); do not silently drop |
| NULL cells | NA / null cell rules aligned with BNData where possible |

- Zero rows → empty DataFrame with columns still present when metadata allows.
- `Query` of non-row SQL (`INSERT`) → Error directing the author to `Exec` (fail closed, teachable).

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

## Implementation boundaries (when accepted — not this document)

| Layer | Responsibility |
| --- | --- |
| Spec / host library docs | Capability surface, Error rules, DataFrame mapping, non-goals |
| Frontend | Typecheck imports and method signatures; no new keywords |
| IR | HOST calls only unless architecture later shares a native binding note |
| `bn_rt` / host | Link or embed SQLite; policy on paths; materialize DataFrame via existing dataframe ABI where possible |
| Support matrix | Native row first; other targets unsupported until proposed |

Production bar: no stub `Open` that always succeeds; no fake Query returning empty frames as “success” without a real engine.

---

## Evolution (after v0)

1. **Prepared statements** — `Prepare` / bind INTEGER|FLOAT|STRING|BOOLEAN; retire encouraging raw string concat.
2. **Explicit `Connection` object** — `Open AS Connection OR Error` if multi-DB programs need it.
3. **BLOB / richer types** — only with DataFrame story locked.
4. **Write DataFrame → table** helper (`InsertFrame`) as sugar over Exec/transactions.
5. Optional **`BNSQLite` module** for higher-level helpers without growing HOST.

---

## Open questions

1. **Open flags:** create-if-missing vs read-only vs read-write — one `Open(path)` with documented default, or `Open(path, mode AS STRING)`?
2. **Capability vs connection object:** methods on `Db` after Open, or `LET c AS Db.Connection OR Error = Db.Open(path)` then `c.Query`?
3. **Result size limit:** hard Error above N rows/bytes vs host config?
4. **Import of BNData:** must every SQLite program import BNData, or does HOST.SQLite re-export a DataFrame type alias (prefer explicit `IMPORT BNData` for honesty)?
5. **Transaction autocommit:** default SQLite autocommit vs requiring Begin for writes in teaching mode?
6. **Registry spelling:** `HOST.SQLite` vs `HOST.Sqlite` — lock to one normalized form.

---

## Decision summary

| Topic | v0 decision |
| --- | --- |
| Keywords | None new |
| Capability | `HOST.SQLite` (`IMPORT … AS Db`) |
| Query return | `Data.DataFrame OR Error` — **caller** assigns with `LET` |
| Open / Exec / tx | `VOID OR Error` (unless Open→Connection is chosen in open Q2) |
| Recordset type | None — use BNData |
| Prepared SQL | Out of v0 |
| Sugar Insert/CreateTable | Optional via `Exec` only in v0 normative surface |

