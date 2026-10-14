# Proposal: object serialization via companion modules (option E)

**Status:** Accepted (option E); S-3/S-4 locked 2026-09-23. **Schedule:** after **0.6.1b** (BNCrypto); delivery bucket [`ongoing/bucket-0.6.1c.md`](../../ongoing/bucket-0.6.1c.md) (closing). **Backend:** interpret (`bni` / `bn_lib_json`) **and** AOT (`bnc` / `bn_rt` + `bn_llvm`) — expanded 2026-09-22.
**Date:** 2026-09-22
**Author (draft):** Quorra for Carlos Quintella
**Depends on (prerequisite, separate claim):** `BNJson` DOM (Object/Array/Get/Set/…) beyond today’s `Parse`/`Stringify` only.
**Non-goals:** reflection; language keyword `SERIALIZE`; automatic field walk; binary formats in the first cut; mixing with `BNData` CSV; interpret-only delivery (AOT is in-scope for 0.6.1c).

---

## 1. Intent

Keep domain `CLASS` types free of I/O and format knowledge. Encoding and decoding live in a **companion module** next to the type (Go-style), imported explicitly. JSON text on the wire uses `BNJson`; persistence/transport use `HOST.FileSystem` / `BNWeb` / etc.

## 2. Shape

For a domain type `Bird` in module `MyApp.Bird`:

| File / module | Owns |
| --- | --- |
| `Bird.bn` (or package export) | Domain only: fields, methods, invariants — **no** JSON |
| `BirdJson.bn` (companion) | `Encode` / `Decode` (and version notes) |

```basic
IMPORT MyApp.Bird AS Bird
IMPORT MyApp.BirdJson AS BirdJson
IMPORT BNJson AS Json

FUNCTION Start() AS VOID
    LET eagle AS Bird.Bird = NEW Bird.Bird("eagle")

    LET doc AS Json.Json OR Error = BirdJson.Encode(eagle)
    IF doc IS Error THEN
        PRINT doc
        RETURN
    END IF

    LET text AS STRING OR Error = Json.Json.Stringify(doc)
    IF text IS Error THEN
        PRINT text
        RETURN
    END IF

    // in
    LET doc2 AS Json.Json OR Error = Json.Json.Parse(text)
    IF doc2 IS Error THEN
        PRINT doc2
        RETURN
    END IF

    LET eagle2 AS Bird.Bird OR Error = BirdJson.Decode(doc2)
    IF eagle2 IS Error THEN
        PRINT eagle2
        RETURN
    END IF
END FUNCTION
```

Naming convention (teaching, not keyword):

- Companion module id: `<Type>Json` when the codec is JSON (e.g. `BirdJson`).
- Entry points: `Encode(value AS T) AS Json.Json OR Error`, `Decode(doc AS Json.Json) AS T OR Error`.
- Optional sugar in the *same companion only*: `EncodeText` / `DecodeText` = Encode/Decode + Stringify/Parse.

## 3. Why this fits Basic Next

- Explicit `IMPORT` — nothing serializes unless asked.
- Domain types stay testable without JSON providers.
- Schema evolution is versioned in the companion (`Encode` can emit `"v": 2`).
- Multiple formats later: `BirdMsgPack.bn` without touching `Bird.bn`.
- No reflection of `PRIVATE` / `HOST` handles.

## 4. Prerequisite: BNJson DOM + AOT parity

Companion modules need to build and read documents without string concatenation:

- Construct: `Object`, `Array`, setters for STRING / INTEGER / FLOAT / BOOLEAN / NULL / nested Json.
- Read: `Get`, discriminant/type query, array length, fail-closed missing/wrong-type → `Error`.
- Keep existing limits (nested depth, size caps) from `language/0.3/bnjson.md`.

**Backend (locked 2026-09-22):** today `BNJson` exists only on the interpret path (`bn_lib_json` via `bni`). `crates/bn_llvm` has no BNJson lowering; `bn_rt` has no JSON ABI. Bucket **0.6.1c** must close that gap: `bn_rt` handle table + Parse/Stringify/DOM ABI, `bn_llvm` emission honoring `bnjson_providers` (same pattern family as `BNData` / `BNLog`), compiler capability rows, and fixtures under both `bni` and `bnc`.

Ship DOM + AOT as the prerequisite claim; companions are examples + convention on top.

## 5. Rejected alternatives (locked preference)

| Option | Status |
| --- | --- |
| A — `eagle.ToJson()` on the class | Rejected (Carlos: dislikes object serialization syntax on the type) |
| B — `Json.Encode(eagle)` generic over interface | Deferred; possible later if interfaces + registration are clean |
| C — text-only one-shot as the only API | Optional sugar inside companion only |
| D — manual DOM only | Remains the *implementation* style inside companions; not the only user story |
| F — keyword / global `SERIALIZE` | Rejected |

## 6. Acceptance sketch

Normative detail lives in [`ongoing/bucket-0.6.1c.md`](../../ongoing/bucket-0.6.1c.md) (objectives **O1–O4**, criteria **A1–A10**, required tests). Sketch:

1. `BNJson` DOM green with fixtures (positive + fail-closed) under **`bni`**.
2. Same DOM + Parse/Stringify surface green under **`bnc`** (native link via `bn_rt`; llvm emission present; capability + parity rows).
3. Example pair `examples/serialization/bird.bn` + `bird_json.bn` (or under `modules/`) round-trips with **`bni run` and `bnc`**.
4. Book appendix: “Companion codecs” — one page, no new keywords; note that JSON companions require BNJson on both interpret and compile paths after this bucket.
5. No language surface change in the first claim (convention + stdlib/`BNJson` only).

## 7. Decisions

| ID | Decision | Status |
| --- | --- | --- |
| S-1 | Companion-module shape (option E) | **Accepted** Carlos 2026-09-22 |
| S-2 | Schedule after BNCrypto | **Accepted** Carlos 2026-09-22 — bucket **0.6.1c** after **0.6.1b** |
| S-3 | Companion naming: `BirdJson` vs `Bird.Json` path | **LOCKED: `BirdJson`** (sibling module, not `Bird.Json` path) — Carlos via bucket recommendations 2026-09-23 |
| S-4 | `EncodeText` / `DecodeText` in v1 | **LOCKED: defer** out of 0.6.1c — companions may add locally; book notes optional — Carlos via bucket recommendations 2026-09-23 |
| S-5 | BNJson AOT in **0.6.1c** (`bn_rt` + `bn_llvm`), not interpret-only | **Accepted** Carlos 2026-09-22 |

### S-3 — LOCKED `BirdJson` (sibling module) — 2026-09-23

`Bird.Json` reads as though the codec belongs to `Bird`, which undermines the one thing
option E exists to teach: the domain type owns no serialization. A sibling id makes the
independence visible in the import line itself.

It also avoids a readability trap. The example already carries `Json.Json.Stringify` —
alias plus class. Introducing `Bird.Json` would put two different `.Json` suffixes in the
same file meaning different things (a codec here, a document type there). `PHILOSOPHY.md`
P2: *"source code is communication between people, not only instructions to a machine."*

### S-4 — LOCKED defer `EncodeText` / `DecodeText` — 2026-09-23

P3: *"Complexity must earn its place through a concrete problem, not anticipation."*

The sugar is cheap in code — it lives in the user's own companion, not in `BNJson` — so
"ship it in v1" really means "teach it in v1", and that doubles what a reader absorbs on
first contact with the convention.

The unsugared path is also the more honest one to teach first: `Encode` then `Stringify`
puts the boundary where the reader can see it, which is P7 (*"boundaries … should not
surprise"*). A companion may add `EncodeText` locally; the book should note that it may,
not present it as part of the convention.

Revisit when someone has written enough companions to be annoyed by the two steps. That is
the concrete problem P3 asks for.

---

**History:**

- **2026-09-22** — Carlos: schedule after crypto (`0.6.1b`); lane **0.6.1c**.
- **2026-09-22** — Carlos chose option E; Quorra drafted this proposal.
- **2026-09-22** — Carlos: expand 0.6.1c — BNJson must ship on `bnc` (`bn_rt` ABI + `bn_llvm` emission), not `bni`-only.
- **2026-09-22** — Quorra: bucket objectives / acceptance / required tests detailed; this sketch defers to the bucket.
- **2026-09-22** — S-3 / S-4 recommendations written and argued from `PHILOSOPHY.md`
  (P2, P3, P7). Both still need Carlos to lock; naming is cheap to change now and
  expensive after the book and fixtures use it.
- **2026-09-23** — Carlos via bucket recommendations: **S-3 LOCKED** (`BirdJson`
  sibling), **S-4 LOCKED** (defer `EncodeText`/`DecodeText` out of 0.6.1c;
  companions may add locally; book notes optional).
