# Proposal: explicit `OVERRIDE` (0.5.2)

**Status:** Proposed — language OO contract (**O2**).
**Date:** 2026-09-14
**Target release:** **0.5.2**
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14):** Require an explicit `OVERRIDE` marker when a subclass method overrides a base method; `bn check` must fail if the base does not declare that method.

**Related:** `EXTENDS`; dynamic dispatch on `PUBLIC` instance methods (0.5.0 language); bucket 0.5.2.
**Non-goals:** multiple inheritance; abstract methods / interfaces-as-abstract beyond existing `IMPLEMENTS` (unless free).

---

## Intent lock (Carlos)

- **`OVERRIDE` explícito** on overriding methods.
- **`bn check` fails** if the named base/ancestor does not declare a matching overridable method.

---

## Candidate surface

```basic
CLASS Animal
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "..."
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC OVERRIDE FUNCTION Speak() AS STRING
        RETURN "woof"
    END FUNCTION
END CLASS
```

### Rules (normative intent)

1. A method that overrides an inherited instance method **must** include `OVERRIDE` (placement: after visibility / `STATIC` as locked in EBNF — recommended: `PUBLIC OVERRIDE FUNCTION Name…`).
2. `OVERRIDE` on a method with **no** matching base declaration → **static error** (e.g. `OVERRIDE_WITHOUT_BASE`).
3. Overriding without `OVERRIDE` when a base method exists → **static error** (e.g. `OVERRIDE_REQUIRED`) — prevents accidental shadowing.
4. Signature compatibility (params/return) must match language rules for overrides (document exact match vs allowed widening in §2 lock).
5. `STATIC` methods: **no** `OVERRIDE` unless Carlos expands (default: static is not virtual).

---

## Acceptance fixtures (sketch)

| ID | Intent |
| --- | --- |
| V1 | Legal `OVERRIDE` + dispatch through base-typed reference |
| V2 | `OVERRIDE` without base → check fails |
| V3 | Base method exists but `OVERRIDE` missing → check fails |
| V4 | Signature mismatch → check fails |

---

## Open (Carlos / Quorra)

1. Exact keyword order with `PUBLIC` / `PRIVATE` / `PROTECTED`.
2. Whether `PRIVATE` base methods are overridable (recommend: **no**).
3. Interface `IMPLEMENTS` methods — require `OVERRIDE` or separate rule?

---

## History

- **2026-09-14** — Carlos: O2 into 0.5.2.
