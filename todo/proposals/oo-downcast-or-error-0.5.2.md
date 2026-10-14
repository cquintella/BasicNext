# Proposal: safe downcast / type test → `T OR Error` (0.5.2)

**Status:** Proposed — language OO contract (**O1**).
**Date:** 2026-09-14
**Target release:** **0.5.2**
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14):** Fill the OO gap for safe narrowing of class references without introducing exceptions.

**Related:** 0.5.0 ARC / class instances; alternative types `T OR Error`; bucket `ongoing/bucket-0.5.2.md`.
**Non-goals:** try/catch; exception types; `instanceof` libraries as the only path.

Nothing here is normative until syntax is locked (§2 of the bucket) and accepted into `docs/0.5.2/` + EBNF + FE.

---

## Intent lock (Carlos)

- Prefer surface that yields **`T OR Error`** (success = narrowed reference; failure = `Error`).
- **No** exception mechanism in 0.5.2.

---

## Candidate syntax (pick one in bucket §2.1)

### Candidate A — preferred sketch (reuse `AS`)

When `expr` has a class type `B` and `C` is a class type such that `C` **EXTENDS** `B` (or is `B`), then:

```basic
LET d AS Dog OR Error = animal AS Dog
IF d IS Error THEN
    PRINT d.Message
ELSE
    d.Bark()
END IF
```

- Upcasts remain ordinary assignment / parameter passing (no `AS` required).
- Narrowing class `AS` is **not** the same as numeric/`STRING` conversion `AS`; diagnostic text must distinguish.
- Failure mode: runtime (or static when provably impossible) → `Error` (stable code TBD, e.g. `TYPE_NARROW_FAILED`).

### Candidate B — explicit keyword

```basic
LET d AS Dog OR Error = TRYCAST animal AS Dog
```

More verbose; avoids overloading `AS`. Use only if Candidate A is rejected.

### Type test without binding (optional sugar)

Not required for claim if `AS` + `IS Error` covers teaching. If added later: must still be Error-free boolean or `T OR Error` — **not** exceptions.

---

## Acceptance fixtures (sketch)

| ID | Intent |
| --- | --- |
| D1 | Successful downcast; method on narrowed type works |
| D2 | Failed downcast → `Error`; no destructor surprises on original |
| D3 | Static reject when target type is unrelated (no inheritance) |
| D4 | Interaction with `AS WEAK` / NULL (document; fail closed) |

Evidence under `docs/superpowers/evidence/oo-downcast-0.5.2/` (names final in bucket §4.2).

---

## Native / ARC

Narrowing must not invent a second object identity. Retain/release rules of 0.5.0 unchanged: downcast shares the same instance; Error path allocates no object.

---

## Open (Carlos / Quorra)

1. Lock Candidate **A** vs **B**.
2. Exact `Error` code / Fluent id.
3. Whether impossible narrowings are always static errors.

---

## History

- **2026-09-14** — Carlos: O1 into 0.5.2; prefer `T OR Error`.
