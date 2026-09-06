# Completion gates — correctness independent of deliverables

> Canonical: `docs/architecture/completion-gates.md`  
> **Locked direction 2026-09-05.**  
> Acyclic DAG and smoke tests are **necessary** and **not sufficient** for semantic consistency.

---

## Problem with weak gates

As written historically in [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md):

- **G2 (IR)** could be “satisfied” by a fixture that only shows an unsupported-for-llvm op failing — without proving language `validate`, negative cases, or a filled support matrix.
- **G3 (hard split)** could pass with **DAG + smoke** `check|run|build` — without proving interpret↔compile consistency, ABI/policy contracts, or absence of semantic types in IR consumers.

`bnc`, process log, and Fluent remain in the **already decided** 0.4.5 scope (Fluent required; `bnc` optional). They do **not** replace the correctness gates below.

---

## Priority sequence (respect existing sprints)

Order work so contracts exist **before** crate cuts claim success:

| Step | Focus | Primary docs / tracks |
| --- | --- | --- |
| **1** | Close **minimum IR contract** + **negative validator** cases (`validate` language rules) | [ir-contract.md](ir-contract.md); bucket §2 / SM5; **GC-IR** below |
| **2** | Fill **support matrix** with evidence; formalize **memory/ABI** + **execution policy** | [support-matrix.md](support-matrix.md), [value-memory-abi.md](value-memory-abi.md), [host-traits.md](host-traits.md); **GC-MX**, **GC-ABI**, **GC-POL** |
| **3** | Consolidate **FrontendSession** (snapshots/SourceId/Revision); eliminate **semantic deps** in IR/backends | [frontend-session.md](frontend-session.md); XM4/XM8; **GC-FE**, **GC-DEP** |
| **4** | **Extract crates**, verifying the contracts above **at each move** | XM8–XM11 / SM6; **GC-EXT** — DAG alone never closes the bucket |

Deliverable tracks in parallel (do not weaken correctness):

| Track | Role |
| --- | --- |
| Fluent / `bn_diag` | Operability — **G4** style gates stay |
| Process log MVP | Observability — already locked |
| `bnc` | Optional UX — must not be the correctness story |

---

## Independent correctness gates (must not be waived by smoke)

| Id | Gate | Passes only when | Insufficient alone |
| --- | --- | --- | --- |
| **GC-IR** | IR contract minimum + **well-formed handoff** | Op/type/`validate` checklist for the slice; **negative** fixtures; backends only consume post-`validate` IR (**W1–W5** in [`../../AGENTS.md`](../../AGENTS.md) / [ir-contract.md](ir-contract.md)) | A single “unsupported llvm” fixture; smoke `run`/`build` alone |
| **GC-SUP** | `validate` ≠ support | Unsupported-for-target fails **`validate_for` / support check** with `TARGET_UNSUPPORTED_*` (or named family) — **not** language-invalid | Collapsing support into `validate` |
| **GC-MX** | Matrix evidence | Structured matrix rows for the claimed subset cite **tests**; EXAMPLE fiction gone for that subset; coverage gap report exists | Markdown “yes” cells without tests |
| **GC-PAR** | Parity beyond stdout | For **each feature/op claimed supported** on a target, run the **pertinent** families (numeric boundaries, errors, observable effects, objects, opt stability) — not a single global “one family somewhere.” Floor for early buckets may stage families, but **announced support ⇒ matching families** — [conformance.md](conformance.md) | Happy-path stdout/exit only; “one extra family anywhere” while claiming a broad subset |
| **GC-ABI** | Value/memory/ABI | Documented categories have fixtures or explicit “not in this release” carve-outs; numeric lowering does not treat LLVM `nsw`/poison as BN `Error` | Extracting `bn_value` alone |
| **GC-POL** | Execution policy | Requirements ≠ support ≠ policy; compiled path has policy carrier + **`bn_rt` re-check** at call boundary for the claimed HOST subset | Build-time allowlist only |
| **GC-FE** | Shared session | Session supports snapshots (incl. unsaved), revision-scoped diagnostics; baseline LSP Problems ≡ `--check` stages | `open(path)` + model cache only |
| **GC-DEP** | No semantic leak | `bn_ir` and backends do not depend on `bn_frontend`/semantic types in the public model | Grep-clean DAG with hidden type reuse |
| **GC-EXT** | Extract with proof | Each crate cut re-runs **GC-IR…GC-DEP** applicable subset + DAG; smoke is additive | `cargo` acyclic + smoke alone |

---

## Mapping onto bucket 0.4.5

| Old gate | Strengthening |
| --- | --- |
| Success claim (DAG + smoke + Fluent) | Keep structural goals; **add** GC-IR, GC-SUP, GC-MX (slice), GC-PAR (expand), GC-DEP for IR cut |
| **G2** | Split: language `validate` negatives (**GC-IR**) **and** support rejection (**GC-SUP**); matrix stub insufficient |
| **G3** | DAG + smoke **plus** contract re-verification (**GC-EXT**); **G3c** = feature-scoped parity families for claimed support (not one family total) |
| **G4** Fluent | Remains; independent of GC-* |
| **G5** `bnc` | Remains optional |

See updated [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md).

## See also

- [milestones-map.md](milestones-map.md)
- [conformance.md](conformance.md)
- [support-matrix.md](support-matrix.md)
- [frontend-session.md](frontend-session.md)
