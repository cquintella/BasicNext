# C-P1.2 Matrix and ABI Release-Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the 0.4.5 LLVM release slice auditable by reconciling emitted `bn_rt` symbols, matrix claims, and ABI ownership evidence.

**Architecture:** Keep `tests/compiler-capabilities.json` as the only support-matrix catalog. Add a deterministic, read-only reconciliation test over the LLVM declaration strings and `bn_rt` C exports; record ownership for every declared symbol in the ABI contract using symbol groups whose membership is machine-checked. The work does not broaden target support: untested operations remain explicitly deferred or uncovered.

**Tech Stack:** Rust workspace (`bn_llvm`, `bn_rt`), Python `unittest`, JSON capability catalog, Markdown architecture contract.

**Spec:** `ongoing/bucket-0.4.5.md` C-P1.2; `ongoing/WBS-0.4.5.md` Activities 3.1–3.2; `docs/architecture/support-matrix.md`; `docs/architecture/value-memory-abi.md`.

## Global Constraints

- Preserve every existing user modification; do not reset, reformat globally, or alter unrelated files.
- `validate_for` remains the sole support-rejection boundary; this work introduces no backend fallback.
- A `llvm-supported` matrix row must cite a real check/run/build fixture; `llvm-deferred` must name its stable rejection diagnostic.
- Reconcile only symbols LLVM declares or emits. A `bn_rt` export not referenced by LLVM is not implicitly a claimed compiled feature.
- ABI ownership distinguishes borrowed inputs, caller-owned out storage, runtime-owned allocations, and opaque handles with their matching release operation.
- All new behavior follows red → green TDD. Do not claim GC-MX or GC-ABI closed outside the verified release slice.
- Do not commit until the relevant sprint or bucket is actually closed and its gates/evidence are complete.

---

## Files and responsibilities

| File | Responsibility |
| --- | --- |
| `tests/test_capabilities.py` | Validates matrix schema, fixture behavior, lowered IR inventory, and new LLVM↔`bn_rt`/ABI inventory invariants. |
| `tests/compiler-capabilities.json` | The single support-matrix record for release-slice fixture claims and explicit deferrals. |
| `crates/bn_llvm/src/llvm/runtime.rs` | Authoritative non-math LLVM `bn_rt` declaration list. |
| `crates/bn_llvm/src/llvm/math.rs` | Authoritative BNMath LLVM `bn_rt` declaration list. |
| `crates/bn_rt/src/*.rs` | Authoritative Rust C ABI exports; production behavior is unchanged unless a declaration/export mismatch is found. |
| `docs/architecture/value-memory-abi.md` | Symbol-group ownership/lifetime contract, with every LLVM declaration assigned exactly one group. |
| `docs/superpowers/evidence/2026-09-06-0.4.5-c-p1-2-matrix-abi.md` | Executable evidence, exact commands, results, declared carve-outs, and any sandbox limitations. |
| `ongoing/bucket-0.4.5.md`, `ongoing/WBS-0.4.5.md` | Accurate status links only after acceptance evidence exists. |

## Task 1: Add the failing LLVM/runtime ABI inventory test

**Files:**
- Modify: `tests/test_capabilities.py`
- Test: `tests/test_capabilities.py::CompilerCapabilityTests::test_llvm_declared_symbols_have_runtime_exports_and_abi_groups`

**Interfaces:**
- Consumes: `BN_RT_DECLS`, `BN_RT_MATH_DECLS`, Rust `#[unsafe(no_mangle)] pub extern "C" fn bn_rt_*` exports, and the `### Complete bn_rt Symbol Ownership for LLVM-Emitted Symbols` table.
- Produces: a sorted, duplicate-free set of declared `bn_rt_*` names; a test failure listing declaration-only symbols, export-only claimed symbols, and ungrouped declarations.

- [ ] **Step 1: Write the failing test.**

  Add helpers that extract `@bn_rt_[A-Za-z0-9_]+` from both LLVM declaration constants, extract C-export names from `crates/bn_rt/src/**/*.rs`, and extract explicit `bn_rt_*` tokens from the ABI ownership table. Assert `declared <= exported` and `declared <= documented`, with sorted missing sets in failures.

- [ ] **Step 2: Run the single test and verify RED.**

  Run: `python3 -m unittest tests.test_capabilities.CompilerCapabilityTests.test_llvm_declared_symbols_have_runtime_exports_and_abi_groups -v`

  Expected: failure identifies currently undocumented individual declarations or insufficiently explicit group membership; it must not fail from a missing test module or an absent binary.

- [ ] **Step 3: Implement the smallest contract representation.**

  Add one fenced `text` inventory directly beneath the ABI ownership table, with one `bn_rt_*` identifier per line and exactly one ownership-group heading per identifier. Do not duplicate the support matrix in a second machine catalog. Expand existing groups only where their ownership rules are already true; split a group when return allocation or invalidation differs.

- [ ] **Step 4: Run the focused test and verify GREEN.**

  Run the command from Step 2. Expected: PASS and no undocumented/missing exported declaration.

## Task 2: Make release-slice matrix claims complete and bounded

**Files:**
- Modify: `tests/compiler-capabilities.json`
- Modify: `tests/test_capabilities.py`
- Test: `tests/test_capabilities.py::CompilerCapabilityTests::test_release_slice_runtime_claims_have_abi_symbols`

**Interfaces:**
- Consumes: existing schema-version-1 matrix rows, the declared-symbol set from Task 1, and fixture IR inventories.
- Produces: explicit `runtime_symbols` (sorted, unique `bn_rt_*` names) for each `provider: "bn_rt"` row and no runtime-symbol claim for `provider: "language"` rows.

- [ ] **Step 1: Write the failing schema test.**

  Require every `bn_rt` row to contain a non-empty sorted unique `runtime_symbols` list; each name must be in the Task 1 declared set. Require language-provider rows to omit `runtime_symbols`. Keep deferred rows bound to `reject_diag`.

- [ ] **Step 2: Run the single test and verify RED.**

  Run: `python3 -m unittest tests.test_capabilities.CompilerCapabilityTests.test_release_slice_runtime_claims_have_abi_symbols -v`

  Expected: failure names the first existing `bn_rt` row without its exact runtime symbol list.

- [ ] **Step 3: Add only verified symbol lists.**

  Populate `runtime_symbols` from LLVM emitted text for the 0.4.5 fixtures: clock, console control, input, BNMath scalar/vector, net resolve, random, and empty DataFrame lifecycle. Do not add rows or symbols merely because they are declared.

- [ ] **Step 4: Verify fixture and catalog behavior.**

  Run: `python3 -m unittest tests.test_capabilities -v`

  Expected: all catalog schema, command, IR-inventory, gap-report, and new ABI linkage checks pass.

## Task 3: Record evidence and synchronize only earned tracker state

**Files:**
- Create: `docs/superpowers/evidence/2026-09-06-0.4.5-c-p1-2-matrix-abi.md`
- Modify: `ongoing/bucket-0.4.5.md`
- Modify: `ongoing/WBS-0.4.5.md`

**Interfaces:**
- Consumes: successful focused tests and documented release-slice carve-outs.
- Produces: reproducible C-P1.2 evidence with commands, exit codes, coverage counts, exact excluded areas, and a truthful tracker status.

- [ ] **Step 1: Add evidence after verification.**

  Record the exact revision, `python3 -m unittest tests.test_capabilities -v`, affected `cargo test` commands, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `git diff --check`. State that network lifecycle tests remain environment-limited only if rerunning confirms it.

- [ ] **Step 2: Update tracker language precisely.**

  Mark C-P1.2 `[X]` only if every LLVM declaration is exported/documented, every `bn_rt` release row lists only declared symbols, and all cited evidence passes. Keep GATE C-P1, G2d, GC-POL, GC-PAR, physical extraction, and unclaimed/deferred API coverage open unless their own evidence exists.

- [ ] **Step 3: Run the completion checks.**

  Run: `cargo fmt --check && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && git diff --check`

  Expected: all commands exit 0. If environment restrictions make a test fail, preserve the failure output in the evidence and leave the related gate open.

## Plan self-review

- Spec coverage: Task 1 implements symbol ownership reconciliation; Task 2 binds matrix claims to real runtime symbols and fixtures; Task 3 produces evidence and truthful bucket/WBS state.
- Scope: no new language surface, no policy carrier, no crate move, no support expansion from declarations.
- Type consistency: all inventory inputs are textual ABI names; the tests operate on sorted Python `set[str]` values and do not introduce a new Rust runtime API.
- Placeholder scan: no deferred implementation instructions or undefined interfaces remain.
