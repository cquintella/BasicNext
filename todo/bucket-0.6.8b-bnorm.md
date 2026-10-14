# Bucket 0.6.8b — BNOrm (typed persistence over BNSqlite)

**Status:** Deferred schedule: **0.6.3** (Carlos 2026-09-21: former **0.5.4**; opened 2026-09-19).
**Depends on:** **0.6.2** claimed — at least **`BNSqlite` (S1) + F1** green. FS/Auth in 0.6.3 are sibling axes; ORM must not tag before S1 is honest. Do **not** tag **0.6.3** until BNSqlite is honest.
**Gate:** Quorra before Carlos.
**Tag policy:** all-or-nothing on the locked claim table — no half-ORM, no “thin demo” that pretends mapping. Prefer *escopo / recorte / versão* (never MVP language).
**Owner (tracker):** TBD (Tron implementation; Quorra gates).
**Policy:** Host still does **not** embed an SGBD. Persistence goes `BNSqlite` → optional `BNOrm`. Other engines (`BNPostgres`, …) are **out of this claim** unless Carlos expands later.

**Authoritative proposals (to author before implementation waves):**
- **R0** — `todo/proposals/bnorm-0.5.4.md` (API locks, mapping contract, what is *not* ORM)
- **R1** — schema / code-generation surface (if kept in claim)
- **Depends on (read-only):** `todo/proposals/bnsqlite-0.5.3.md` (S1/F1); BNData DataFrame ownership ABI

**Supersedes / does not reopen:** CLI `sqlite3` via `HOST.Exec`; direction A `HOST.SQLite`; reflection-heavy / lazy / magic ORMs.

---

## Intent (maximum ambition, fine scope deferred)

Ship a **low-friction, high-transparency** persistence layer for Basic Next:

- SQL remains **visible** (no hidden query DSL that invents SQL the programmer cannot read).
- Types remain **explicit** (`STRUCT` / `CLASS`, `T OR Error`, no silent NULL coercion).
- Mapping is **mechanical and total** where claimed (column ↔ field), fail-closed on mismatch.
- Boilerplate may be **generated**, but runtime stays boring and auditable.
- Default path: **BNSqlite + BNData DataFrame** underneath — Orm does not re-implement a driver.

**Fine claim rows, fixture IDs, and EBNF deltas wait** until BNSqlite’s shipped surface is frozen. This bucket records the **ceiling** Carlos wants in 0.6.4 so planning and proposals can start in parallel without pretending S1 is done.

---

## Ambition ceiling (claim candidates — maximize)

Everything below is **in scope to pursue** for 0.6.4 unless cut by Carlos/Quorra when BNSqlite lands. Detail locks = proposal R0/R1 after S1.

### A — Core mapping (must-have spirit)

| ID | Item | Spirit |
| --- | --- | --- |
| **A1** | Entity declaration | Domain `STRUCT` or `CLASS` with explicit field ↔ column contract (names, types). No annotation soup if a clearer BN surface exists; if attributes/`AS` metadata are needed, lock in R0. |
| **A2** | `FromRow` / `ToRow` | Bidirectional map `DataFrame` row (or named columns) ↔ entity. Fail-closed: missing column, type mismatch → `Error`. |
| **A3** | `FromFrame` / `ToFrame` | Whole-frame map ↔ vector of entities (or fail on first bad row — lock policy in R0). |
| **A4** | Parameter binding | Typed params for SQL (`INTEGER`, `FLOAT`, `STRING`, `BOOLEAN`, `NULL`) — no string-concat SQL for values. |
| **A5** | Identity / primary key | Explicit PK field; `FindById`, insert returning id where SQLite allows — contract after BNSqlite. |

### B — Repository surface (low friction at call site)

| ID | Item | Spirit |
| --- | --- | --- |
| **B1** | `Repository[T]` pattern | `Get`, `Find`, `Insert`, `Update`, `Delete` (or BN-idiomatic names) returning `T OR Error` / `VOID OR Error` / frame. |
| **B2** | SQL stays in the open | Repository methods take or own **visible** SQL strings (or named query constants). Orm does not invent a second language. |
| **B3** | Transactions | `Begin` / `Commit` / `Rollback` through BNSqlite; repository helpers that accept an open connection/tx. |
| **B4** | Connection lifetime | Open/Close discipline aligned with ARC / `Close` (not `DELETE`). Document who owns the connection. |

### C — Relations (ambitious, still explicit)

| ID | Item | Spirit |
| --- | --- | --- |
| **C1** | Explicit load | `LoadPosts(user)` / `FindWithAuthor(id)` — programmer asks; **no lazy proxies**. |
| **C2** | Join → entities | Map join results into nested `STRUCT`s or sibling entities with explicit column prefixes — fail-closed. |
| **C3** | Collections | One-to-many as `VECTOR OF T` filled by a second query or a single join strategy locked in R0. |

### D — Schema & generation (maximum automation without magic runtime)

| ID | Item | Spirit |
| --- | --- | --- |
| **D1** | Schema as data | Table/column/PK/FK description (BN structs or sidecar) that Orm and generators share. |
| **D2** | Codegen CRUD | Generate entity `STRUCT`, `FromRow`/`ToRow`, and repository stubs from schema **or** from SQLite `PRAGMA` introspection (pick one primary; other may be follow-on). |
| **D3** | Migrations (honest) | Versioned `.sql` (or BN-driven) up/down scripts applied via BNSqlite `Exec` — **visible SQL**, recorded version table. Not an opaque auto-diff engine in 0.6.4 unless Carlos locks it. |
| **D4** | `bn orm` CLI (optional in claim) | `bn orm generate` / `bn orm migrate` if it fits the toolchain without a second binary — otherwise scripts under `scripts/`. |

### E — Query helpers (friction cutters, not a second SQL)

| ID | Item | Spirit |
| --- | --- | --- |
| **E1** | Named queries | Module-level constants or `QUERY` registry — still SQL text. |
| **E2** | Filter builders (optional) | Only if they **print** the SQL they build (debuggable); otherwise cut. Prefer raw SQL + params. |
| **E3** | Pagination / order helpers | Thin wrappers that append `LIMIT`/`OFFSET`/`ORDER BY` with typed args. |

### F — Safety & policy

| ID | Item | Spirit |
| --- | --- | --- |
| **F1** | Fail-closed mapping | Strict mode default; no silent drop of columns. |
| **F2** | Deny in restricted hosts | Same posture as BNSqlite / Exec — sandboxed profiles cannot open arbitrary paths unless allowed. |
| **F3** | SQL injection | Values only via binding API; document that string-built SQL is programmer responsibility. |
| **F4** | Diagnostics | Stable codes for map/bind/constraint failures (align with expressive diagnostics). |

### G — Book, examples, fixtures

| ID | Item | Spirit |
| --- | --- | --- |
| **G1** | Book chapter / appendix | Teach Orm as “explicit map + visible SQL”, contrast with heavy ORMs. |
| **G2** | Examples | Blog/users schema: insert, get, list, update, delete, one join, one transaction. |
| **G3** | Fixtures-first | Suite **O1…On** (IDs assigned when R0 locks) green on interpret; native if BNSqlite native is in claim. |

### H — Explicit non-goals (still list them so they are not smuggled in)

- Lazy loading / identity map that mutates behind the programmer’s back
- Full ActiveRecord base class that hides SQL for all CRUD forever
- Multi-backend Orm in the same claim (`BNPostgres` etc.)
- Runtime reflection over arbitrary classes without a schema contract
- CLI `sqlite3` subprocess path

---

## Architecture locks (provisional)

| Lock | Implication |
| --- | --- |
| BNSqlite underneath | Orm **imports** BNSqlite (+ BNData); does not open sockets to a private driver |
| Spec > interpret > compile | Interpret is reference for mapping; LLVM only if native path claimed |
| Transparency | Generated code is checked in or clearly regenerated — no mystery |
| Errors | `T OR Error` / `VOID OR Error`; no exceptions |
| Close, not DELETE | Connection/statement lifecycle follows 0.5.0+ DNA |
| All-or-nothing | Do not ship “mapping only for INTEGER columns” as 0.6.4 complete |

---

## Critical path (coarse — refine after S1)

1. **Wait / gate** — 0.6.3 BNSqlite S1+F1 claimed.
2. **R0 proposal** — freeze entity/repo/map API against real BNSqlite signatures.
3. **Fixtures-first** — O* programs committed red, then green.
4. **A\* + B\*** implement on interpret.
5. **C\*** relations explicit-only.
6. **D\*** codegen + migrations as locked.
7. **G\*** book + examples; Quorra; Carlos tag.

---

## Hard acceptance gate (skeleton)

Do **not** tag 0.6.4 until:

1. Pré-req 0.6.3 BNSqlite claimed.
2. R0 (and R1 if codegen in claim) locked by Carlos/Quorra.
3. Claim table finalized (subset of A–G above) with fixture IDs.
4. All claim fixtures green (interpret; native if required).
5. Book/examples match shipped surface.
6. Quorra gates + Rust evidence + Carlos tag.

*(Exact checkbox matrix filled when claim rows are locked.)*

---

## Open questions (do not block the bucket file)

1. Entity surface: plain `STRUCT` + conventions vs dedicated `ENTITY` syntax.
2. Codegen primary input: SQLite schema introspection vs BN schema files.
3. How much of C/D/E survives the post-S1 cut without diluting the tag.
4. Module name: `BNOrm` vs `BNPersist` vs nested `BNSqlite.Orm` — **lean BNOrm** until Carlos says otherwise.

---

## History

- **2026-09-21** — Carlos invert: this bucket is **0.6.3** (was numbered 0.6.4).

- **2026-09-21** — Carlos: reprogrammed former **0.5.4** (briefly numbered 0.6.4 before invert) → **0.6.4** (after 0.6.3 BNSqlite).

- **2026-09-19** — Carlos lock: ORM is **0.6.4**; SQLite+Files+Web Auth stay in **0.6.3**.


- 2026-09-19 — Carlos: open **0.6.4** as Orm-only bucket; put the **maximum** ambition in; fine scope deferred because BNSqlite (0.6.3) is not ready; CLI sqlite path remains discarded; prefer low-friction explicit mapping + visible SQL over magic ORM.

## Triage (2026-10-09)

Not implemented: no `BNOrm` module under `modules/bn/`, no crate, no spec
file. Its dependency `BNSqlite` shipped (`done/bucket-0.6.4-bnsqlite.md`).
Rescheduled after 0.6.7 as 0.6.8b; the 0.6.3 number went to the Error-code
bucket (`done/bucket-0.6.3.md`).
