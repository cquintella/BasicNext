# Support matrix — verifiable contract (to-be)

> Canonical: `docs/architecture/support-matrix.md`  
> **Status:** **contract direction locked 2026-09-05**; a bounded 0.4.4
> subset is claimed with executable evidence. Unlisted combinations remain
> explicitly unclaimed.
> A markdown table of “addition = yes” is **not** enough to sustain releases.

---

## Purpose

Record, in a form that tools and CI can check, which **BN IR operations** (under which **type / condition** constraints) each **target/provider** supports, what **diagnostic** fires on reject, and which **tests** prove it.

This matrix is what makes “compile implements a documented subset” ([conformance.md](conformance.md)) **auditable**. Until rows are real and gated, llvm releases must not claim matrix-backed coverage.

Related: [ir-contract.md](ir-contract.md), [value-memory-abi.md](value-memory-abi.md), [host-traits.md](host-traits.md) (target support ≠ execution policy), bucket 0.4.5 §2 / XM0–XM10.

---

## Why a coarse “supported” cell fails

A line like “integer addition — supported” is insufficient without:

- **Which types** (e.g. `INTEGER` widths, `FLOAT`, vectors).
- **Which conditions** (checked overflow vs wrap; const vs runtime; HOST-gated).
- **Which targets** (interpret, llvm/native, wasm32, …).
- **How rejection is reported** (stable diagnostic code — and whether it is a **language** error or a **target-support** rejection).
- **Which tests** lock the row.

Without that, CI cannot prove coverage and reviewers cannot tell poison/`nsw` hazards from language meaning ([value-memory-abi.md](value-memory-abi.md)).

---

## Structured source of truth (required shape)

Prefer one **machine-readable** catalog (TOML/YAML/JSON — exact format open, AQ-08) as the source; generate human docs and coverage reports from it. Each **row** (or record) must associate at least:

| Field | Meaning |
| --- | --- |
| **`op`** | IR opcode / HOST op / feature id (stable id, not marketing prose) |
| **`type_constraints`** | Operand/result types or type classes this row applies to |
| **`conditions`** | Extra predicates (overflow mode, const-eval only, requires HOST.X, …) |
| **`target`** | `interpret` \| `llvm-native` \| `wasm32` \| … |
| **`provider`** | Optional provider/profile when target support depends on it |
| **`support`** | `yes` \| `no` \| `partial` (partial **requires** subset text) |
| **`reject_diag`** | Diagnostic id when unsupported on this target (must be a **support** code family, not a fake language error) |
| **`tests`** | List of test ids / paths that must pass for this row to count as covered |

Documentation markdown and coverage dashboards are **views** of this catalog, not the other way around.

### Record shape

```toml
[[entry]]
op = "binop.add"
type_constraints = ["INTEGER"]
conditions = ["overflow = checked"]  # must match language + lowering contract
target = "llvm-native"
support = "partial"
reject_diag = "TARGET_UNSUPPORTED_OP"   # support rejection — NOT a language TYPE_MISMATCH
tests = ["tests/test_compiler_parity.py::…", "tests/…/overflow_checked.bn"]
notes = "Must not lower checked overflow as bare add nsw (poison ≠ BN Error)"
```

---

## Separate `validate` from target-support checking

| Check | Question | Failure means |
| --- | --- | --- |
| **`validate` (language IR)** | Is this IR well-formed for the **language**? | Program/IR is **invalid** — language/Frontend/IR contract broken |
| **`validate_for(target)` / support check** | Does **this backend** implement this IR under the matrix? | Program may be **valid BN**; this **target** cannot (yet) run/compile it |

**Normative:** an LLVM (or wasm) limitation must **not** be reported as if the program violated the language. Use a distinct diagnostic family for target-support rejection (name TBD; e.g. `TARGET_UNSUPPORTED_*`). Language errors stay language errors.

DFD **3.2 Validate IR** is the language/structural gate. Support filtering for compile profiles is a **later/sibling** gate (Control/compile path / `validate_for`), driven by this matrix — see [ir-contract.md](ir-contract.md).

---

## Release gate (before claiming matrix-backed releases)

A release that advertises llvm/subset support should not ship until:

1. EXAMPLE/fiction rows are gone from the normative catalog view.
2. Every `support = yes|partial` row for that target cites **≥1** automated test.
3. Coverage tooling can list **ops×types×targets** without tests (gap report).
4. `validate` vs support-check diagnostics are distinguishable in fixtures.
5. Conformance gates below cover more than happy-path stdout (next section).

Until then, status remains **stub data** even though this **contract shape** is locked.

---

## Conformance / parity gates (expand beyond stdout)

Existing differential tests ([`../../tests/test_compiler_parity.py`](../../tests/test_compiler_parity.py), already in CI) mostly compare **return code + stdout** on supported constant-ish programs. That is necessary and should grow, but it is **not** sufficient evidence of interpret↔compile consistency.

**Expand gates** (matrix-linked) to cover at least:

| Gate family | What to compare / assert |
| --- | --- |
| **Numeric boundaries** | Overflow/underflow, widths, euclidean div/rem, power/shift — per [value-memory-abi.md](value-memory-abi.md); never assume LLVM `nsw`/poison == BN Error |
| **Errors** | Language `Error` values, traps, and toolchain failures — distinct; stderr/diagnostic codes where applicable |
| **Observable effects** | Console I/O, intentional HOST side effects under policy — not only stdout of pure prints |
| **Objects** | Identity/aliasing, `DELETE`/handles, method/interface dispatch observables |
| **Optimizations** | Opt levels must not change language-visible results on the support subset (or matrix marks opt-sensitive rows) |

Happy-path stdout/exit alone leaves those dimensions **without evidence**.

Cross-backend diffs remain comparison **B** in [conformance.md](conformance.md); each gate also needs comparison **A** (spec-derived expectations) where fixtures exist.

---

## Bounded 0.4.4 claimed subset

These rows project the machine-readable catalog in
[`../../tests/compiler-capabilities.json`](../../tests/compiler-capabilities.json).
They claim only the exact fixture/type/condition combinations exercised there;
other uses of the same operation remain unclaimed.

| Stable row | Target | Constraint / evidence |
| --- | --- | --- |
| `cap.start-exit-code` | interpret, llvm-native | `Start` returns `INTEGER`; `start-exit-code.bn` |
| `cap.print-integer` | interpret, llvm-native | `INTEGER` literal output; `print-integer.bn` |
| `cap.print-float` | interpret, llvm-native | `FLOAT` literal output; `print-float.bn` |
| `cap.euclidean-div-rem` | interpret, llvm-native | Euclidean division/remainder fixtures |
| `cap.power-shift` | interpret, llvm-native | integer/float power and shift fixture |
| `cap.clock` | interpret, llvm-native | clock predicate fixture |
| `cap.console-control` | interpret, llvm-native | `HOST.Console` control output fixture |

The catalog also records explicit `llvm-deferred` rows for valid programs whose
interpreter behavior is known but whose LLVM lowering is not claimed. Wasm and
provider-specific combinations are unclaimed in this release slice.

---

## See also

- [conformance.md](conformance.md)
- [open-questions.md](open-questions.md) — AQ-08
- [glossary.md](glossary.md)
