# Proposal: multiple construction via STATIC factories — path A (0.5.2)

**Status:** Proposed — language OO / documentation contract (**O4**).
**Date:** 2026-09-14
**Target release:** **0.5.2**
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14):** Support multiple construction patterns without rushing CONSTRUCTOR overloads.

---

## Intent lock (Carlos)

- **Path A (preferred — in claim):** document and test **`PUBLIC STATIC` factory functions** as the normative pattern for alternate construction.
- **Path B (out of claim unless new lock):** `CONSTRUCTOR` overloads — **do not** implement in the same wave without an explicit Carlos lock.

---

## Path A — normative pattern

Today: one `FUNCTION CONSTRUCTOR(…)`. Alternate entry points:

```basic
CLASS Temperature
    PRIVATE celsius AS FLOAT

    FUNCTION CONSTRUCTOR(celsius AS FLOAT)
        SELF.celsius = celsius
    END FUNCTION

    PUBLIC STATIC FUNCTION FromCelsius(c AS FLOAT) AS Temperature
        RETURN NEW Temperature(c)
    END FUNCTION

    PUBLIC STATIC FUNCTION FromFahrenheit(f AS FLOAT) AS Temperature
        RETURN NEW Temperature((f - 32.0) * 5.0 / 9.0)
    END FUNCTION
END CLASS
```

### Deliverables for O4 claim

1. Proposal + book/OO section: factories as the supported multi-construction style.
2. Examples under `examples/` or evidence `C*`.
3. Tests proving factories work with 0.5.0 ARC (single destructor, no leak).
4. Explicit statement that **CONSTRUCTOR overload is not part of 0.5.2**.

---

## Path B — deferred

If Carlos later locks overloads:

```basic
FUNCTION CONSTRUCTOR(celsius AS FLOAT)
FUNCTION CONSTRUCTOR(f AS FLOAT, scale AS STRING)
```

Requires separate EBNF, overload resolution rules, and FE work — **not** in this proposal’s acceptance.

---

## Acceptance fixtures (sketch)

| ID | Intent |
| --- | --- |
| C1 | Two STATIC factories return distinct configurations |
| C2 | ARC: one DEINIT per instance from factory |
| C3 | Documented reject: attempting second CONSTRUCTOR signature fails parse/check (proves path B absent) |

---

## History

- **2026-09-14** — Carlos: O4 path A into 0.5.2; path B only with later lock.
