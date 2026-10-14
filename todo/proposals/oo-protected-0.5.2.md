# Proposal: `PROTECTED` visibility (0.5.2)

**Status:** Proposed — language OO contract (**O3**).
**Date:** 2026-09-14
**Target release:** **0.5.2**
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14):** Visibility between `PUBLIC` and `PRIVATE` — visible inside the `EXTENDS` hierarchy, not to the outside world.

**Related:** existing `PUBLIC` \| `PRIVATE`; `EXTENDS`; bucket 0.5.2.
**Non-goals:** friend modules; package-private across folders; multiple inheritance.

---

## Intent lock (Carlos)

- **`PROTECTED`**: visible in the inheritance hierarchy; **not** visible to unrelated code.

---

## Candidate surface

```basic
CLASS Animal
    PROTECTED FUNCTION Breath() AS VOID
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION Run() AS VOID
        Breath()   // OK — subclass
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET d AS Dog = NEW Dog()
    // d.Breath()  // NOT OK — outside hierarchy
END FUNCTION
```

### Access matrix (normative intent)

| Access site | `PUBLIC` | `PROTECTED` | `PRIVATE` |
| --- | --- | --- | --- |
| Same class | yes | yes | yes |
| Subclass (`EXTENDS`) body | yes | yes | no |
| Other module / unrelated type | yes | no | no |

- `PROTECTED` applies to methods and fields (same places `PUBLIC`/`PRIVATE` apply today).
- EBNF: extend `visibility` to `"PUBLIC" | "PRIVATE" | "PROTECTED"`.
- Reserved word: add `PROTECTED`.

---

## Acceptance fixtures (sketch)

| ID | Intent |
| --- | --- |
| P1 | Subclass reads/calls `PROTECTED` member — OK |
| P2 | Unrelated function/module access — static error |
| P3 | `PRIVATE` still invisible to subclass |
| P4 | Interaction with `OVERRIDE` of `PROTECTED` methods (with O2) |

---

## Open (Carlos / Quorra)

1. Can `PROTECTED` appear on `STATIC` members? (recommend: allow, hierarchy-only access from subclass static/instance as locked).
2. Visibility of `PROTECTED` through a base-typed reference from outside (`Animal` ref calling `PROTECTED`) — recommend: **no** (only from subclass body / same class).

---

## History

- **2026-09-14** — Carlos: O3 into 0.5.2.
