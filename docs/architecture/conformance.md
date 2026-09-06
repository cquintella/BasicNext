# Conformance model (spec → reference → backends)

> Canonical: `docs/architecture/conformance.md`  
> Locked direction **2026-09-05** with [target-architecture.md](target-architecture.md) principle 1.

## Hierarchy

```text
Language specification (docs/language/…)
        │  defines behaviour
        ▼
Interpreter / bn_runtime   ← executable REFERENCE (subordinate to the spec)
        │  should match the spec
        ▼
Compiler / bn_llvm         ← equivalent on the documented SUPPORT SUBSET
```

- The **specification** is normative for what programs mean.
- The **interpreter** implements an **executable reference**. It is the preferred way to *observe* behaviour in tests, but a reference bug is a **defect**, not a license for the compiler to copy the bug forever.
- The **compiler** must match the specification on the [support-matrix.md](support-matrix.md) subset — not “whatever the interpreter did last Tuesday,” and not LLVM `lli` as language meaning.

## Two comparisons (required)

| # | Comparison | Purpose |
| --- | --- | --- |
| **A** | Each backend vs **expected results derived from the specification** | Spec remains the authority; fixtures/oracles of expected output, diagnostics, and exit codes come from the language rules (and published matrices), not from “diff against whatever bn printed yesterday” alone. |
| **B** | Backends **against each other** (interpret vs compile) on the shared subset | Catches accidental divergence between execution modes. |

Both are required. **B alone is dangerous**: Frontend and lowering are **shared**, so interpret and compile can **agree on the same wrong answer** (or the same static error) when the bug sits above both backends. Cross-backend agreement is necessary evidence, not sufficient proof of conformance.

Static diagnostics from the shared Frontend (2.0–3.0) are one place where both backends “agree” by construction; runtime/result equivalence still needs **A** and **B** on execute/compile outcomes.

## Practical CI shape (intent)

1. Spec-derived fixture suite → run with **interpret**; fail on mismatch to expected.
2. Same suite (**support-matrix** filtered) → **compile** + run artifact; fail on mismatch to expected.
3. Where both apply, also **diff** interpret vs compile results (and selected diagnostics) to catch drift.

### Existing parity tests

[`../../tests/test_compiler_parity.py`](../../tests/test_compiler_parity.py) (CI) already diffs interpret vs compile on **return code + stdout** for a fixture list. Keep and grow it; **do not** treat it as complete conformance.

### Required gate expansion (matrix-linked)

| Family | Evidence beyond happy-path stdout |
| --- | --- |
| Numeric boundaries | Overflow/widths/div-rem/shifts; lowering must match [value-memory-abi.md](value-memory-abi.md) |
| Errors | `Error` vs trap vs internal failure; diagnostic codes where applicable |
| Observable effects | HOST/console side effects under policy |
| Objects | Aliasing, `DELETE`/handles, dispatch observables |
| Optimizations | Opt levels must not change language-visible results on the support subset (or matrix marks exceptions) |

**Claim rule:** each **announced** support row needs the **pertinent** families above. Shrink the matrix to stage; do not under-test announced support ([completion-gates.md](completion-gates.md) GC-PAR).

Support-matrix rows that claim `support = yes` for a target must cite tests from these families where relevant — see [support-matrix.md](support-matrix.md).

See also: [ir-contract.md](ir-contract.md), [support-matrix.md](support-matrix.md), [semantic-analysis.md](semantic-analysis.md), [value-memory-abi.md](value-memory-abi.md).
