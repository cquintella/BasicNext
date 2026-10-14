# Proposal: qualified export import — `IMPORT Module.Export AS Alias` (0.5.2)

**Status:** Proposed — language / module-system contract.
**Date:** 2026-09-13
**Target release:** **0.5.2** (after 0.5.0 ARC + typed `AWAIT`, and after 0.5.1 `bn eval` + `HOST.Exec` claim).
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-13):** Allow importing a **specific exported resource** from a module under its own alias, e.g. `IMPORT BNString.String AS Str`, without pulling the whole module under one alias only, and without `FROM` syntax.

**Related (do not conflate):**
- Current EBNF: `IMPORT` *path* `AS` *identifier* — dotted *module* paths mean nested module files today (`BNString.String` → `BNString/String.bn`), **not** an export inside `BNString.bn`.
- 0.4 rule: import paths are **logical module names**, not filesystem `.bn` strings in source.
- 0.5.1 bucket — `bn eval`, `HOST.Exec`; **this feature is 0.5.2**, not 0.5.1.
- Future `BNString.Regex` / `BNRegex` — consumers benefit from qualified import once Regex exists; this proposal is the **import mechanism**, not Regex itself.

Nothing here is normative until accepted into `docs/0.5.2/` (or language book), EBNF, Frontend resolution, and fixtures.

---

## Locked syntax (Carlos 2026-09-13)

```basic
IMPORT BNString.String AS Str
IMPORT BNString.Regex AS Re
IMPORT BNString AS S
```

| Form | Meaning |
| --- | --- |
| `IMPORT M AS A` | Whole module (unchanged). Use `A.Export`. |
| `IMPORT M.E AS A` | **Qualified export import:** `E` is an `EXPORT` of module `M`; alias `A` denotes that export only. |

- `AS` remains **required**.
- No `FROM` keyword in this proposal.
- No `.bn` extension in the import path (logical names only).
- Selective alias does **not** expose sibling exports (`Str` is `String` only, not `Tokenizer`).

---

## Semantics

1. Resolve logical module `M` exactly as today (`modules/bn/BNString.bn` for `BNString`, user `modules/…`, etc.).
2. If the import path has **one** segment (`IMPORT BNString AS S`): current whole-module behaviour.
3. If the path has **two or more** segments (`IMPORT BNString.String AS Str`):
   - **New rule (0.5.2):** let `M` = all segments except the last, `E` = last segment.
   - Load module `M`. If `E` is a public `EXPORT` (class, function, interface, const — same export rules as member access through a module alias), bind alias `Str` to that export.
   - If `E` is **not** an export of `M`, emit a static error (e.g. `IMPORT_EXPORT_NOT_FOUND`).
4. **Disambiguation with nested modules:** if a nested module file `M/E.bn` (or equivalent nested logical module) **also** exists, the language must define priority. **Recommended lock:** prefer **export of parent module `M`** when `E` is an `EXPORT` of `M`; only if `E` is not an export, fall back to nested-module resolution (today’s behaviour). Document the rule; add a fixture where both would collide and show the chosen outcome.
5. `HOST.Capability` paths stay on the Host production (`IMPORT HOST.Net AS Net`); this proposal does not redefine Host. Optional later: `IMPORT HOST.Net.Address AS Addr` only with an explicit Host sub-resource contract (out of 0.5.2 minimum unless free).

### Example

```basic
IMPORT BNString.String AS Str
IMPORT BNString.Tokenizer AS Tok

FUNCTION Start() AS VOID
    LET s AS Str = NEW Str("a,b,c")
    LET t AS Tok OR NULL = Tok.New(s.Value(), ",")
END FUNCTION
```

(`Value()` / exact `BNString.String` API as shipped — illustrative.)

Whole-module form remains valid:

```basic
IMPORT BNString AS S
' S.String , S.Tokenizer
```

---

## Non-goals (0.5.2)

- `IMPORT X FROM M AS A` (explicitly **not** chosen; Carlos locked dotted form).
- Writing `BNString.bn` in the import path.
- Injecting exports into the global namespace without `AS`.
- Multi-export list in one line (`IMPORT BNString.String, BNString.Regex AS …`).
- Implementing `Regex` itself (separate proposal/module work).
- Changing `EXPORT` visibility rules beyond what qualified import needs.

---

## Implementation sketch

| Layer | Work |
| --- | --- |
| EBNF / language doc | Document two-segment (N-segment) meaning: last = export candidate |
| Frontend module loader | After resolving parent module, look up export `E` before nested file |
| Diagnostics | `MODULE_NOT_FOUND` vs `IMPORT_EXPORT_NOT_FOUND` |
| Book `docs/book/en/05_…` | Examples; note 0.5.2 |
| Fixtures | `docs/superpowers/evidence/import-qualified-0.5.2/` |

---

## Acceptance

| ID | Intent |
| --- | --- |
| Q1 | `IMPORT BNString.String AS Str` typechecks; `NEW Str(...)` works when `String` is exported |
| Q2 | `IMPORT BNString.NoSuch AS X` → static error (not silent nested-path miss only) |
| Q3 | `IMPORT BNString AS S` unchanged |
| Q4 | Alias does not see sibling exports |
| Q5 | Nested-module vs export priority documented + fixture |
| Q6 | Interpret reference green; native same observables or honest matrix row |

---

## Changelog

- **2026-09-13** — Carlos: lock `IMPORT BNString.String AS Str` form; target **0.5.2** (not 0.5.1).

## History

- **2026-09-14** — Bucket 0.5.2 expanded (O1–O4); I1 remains in the same claim EAP.
