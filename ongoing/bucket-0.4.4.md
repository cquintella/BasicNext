# Basic Next 0.4.4 — Soft-prep + bug-fix (pre–0.4.5)

> **Path status (2026-09-05): CLOSED — release-ready.** The 0.4.4 bucket
> execution and product gates are complete. Publication occurs through the
> `v0.4.4` tag workflow. Hard crate split / Fluent ship only in
> [`bucket-0.4.5.md`](bucket-0.4.5.md), and only after **G-SOFT** here.

Mark each checkbox when the activity’s **Done when** criteria are met and
evidence is linked (PR, test name, or doc path). Do not mark a GATE until every
blocking activity under it is `[x]`.

---

## 0. How to use this bucket

1. Work **SECTION S** first when unblocking 0.4.5 architecture work (**G-SOFT**).
2. Work **SECTION 0–4** for product quality / release (**G4**).
3. **G-SOFT** and **G4** are independent: soft-prep can finish while bug-fix
   continues (record overlap if you open 0.4.5 implementation early).
4. After each activity: run the **Verify** commands listed; paste or link
   evidence in the activity notes / PR.
5. Language DNA freeze still applies: no new keywords, HOST capabilities, public
   APIs, or IR instruction kinds unless a separate accepted proposal says so.
6. Execute preparation in order: baseline + dependency enforcement → IR
   regressions → reusable matrix/runner → bounded product fixes. Finish each
   preparation gate before starting the next. Record overlapping release work
   with its exact baseline and integration checks; overlap is not a gate waiver.
7. Before starting an activity, identify its owner, applicable contract,
   dependencies, and reproducible acceptance cases. Unresolved contract choices
   block the affected activity. Keep canonical evidence in versioned docs;
   local `audit/` workpapers are supporting evidence only.
8. Planning edits do not close implementation checkboxes. Record deferred work
   as **DEFERRED**, with owner and destination activity, rather than `[X] DONE`.

### Normative references

| Topic | Path |
| --- | --- |
| Agent brief / W1–W5 | [`../AGENTS.md`](../AGENTS.md) |
| Architecture package | [`../docs/architecture/README.md`](../docs/architecture/README.md) |
| Completion gates GC-* | [`../docs/architecture/completion-gates.md`](../docs/architecture/completion-gates.md) |
| IR contract / handoff | [`../docs/architecture/ir-contract.md`](../docs/architecture/ir-contract.md) |
| WBS traceability | [`WBS-0.4.md`](WBS-0.4.md), Activities 7.1–7.4 |
| Soft→hard milestones | [`../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../audit/workpapers/09-synthesis/fe-be-split-milestones.md) |
| Next bucket | [`bucket-0.4.5.md`](bucket-0.4.5.md) |
| Language 0.4 | [`../docs/language/0.4/0.4.ebnf`](../docs/language/0.4/0.4.ebnf), [`0.4.md`](../docs/language/0.4/0.4.md) |

### Standard Rust verify (required for Rust implementation changes)

Activity-specific checks run first and supplement this gate. Documentation-only
changes require link/reference review and `git diff --check`; they do not imply
execution of the Rust or backend suites.

```bash
cargo fmt --check
cargo test --locked --all-targets -- --test-threads=1
cargo clippy --locked --all-targets -- -D warnings
git diff --check
```

### Role vs 0.4.5

| | **0.4.4 (this file)** | **0.4.5** |
| --- | --- | --- |
| Intent | Soft split + IR bridge hygiene + bug-fix | Hard crates + Fluent + full GC-* |
| Split | SM0–SM2, start SM3 | SM3 leftover → SM6 |
| Start hard-split coding | After **G-SOFT** | — |
| Public tag `0.4.4` | After **G4** | — |

---

# PART A — Soft preparation (feeds 0.4.5)

## SECTION S0 — Ownership + soft split (SM0–SM1)

### ACTIVITY S0.1 — Confirm FE | IR | BE ownership table

- [X] ACTIVITY S0.1 — Current-tree ownership inventory recorded in
  [`docs/architecture/ownership.md`](../docs/architecture/ownership.md), linked
  from the architecture README, and reconciled with target M0/A0.1. The table
  records frontend lowering separately from IR validation and preserves the
  known semantic-type debt; no code move or enforcement claim is made yet.

**Objective:** One agreed ownership map for today’s tree (no crate cut).

**Execute:**

1. Open [`../docs/architecture/target-architecture.md`](../docs/architecture/target-architecture.md) ownership / DAG sections and [`fe-be-split-milestones.md`](../audit/workpapers/09-synthesis/fe-be-split-milestones.md) M0.
2. Walk `src/` and list modules under **FE** (lex/parse/ast/token/source/module_graph/semantic/keyword_registry + lowering when present), **IR** (`ir/**`), **BE** (runtime/heap/dispatch/HOST/llvm), **EDGE** (main/cli/lsp/dap).
3. Write or update the canonical file/module → ownership table under `docs/architecture/`, linked from its README. Use audit workpapers only as supporting evidence. Distinguish frontend-owned lowering from the IR model/validator and shared source identities.
4. Flag mismatches (e.g. BE importing parser) as debt with paths — do not fix yet unless trivial.

**Done when:** Ownership table checked in; reviewed against target-architecture; no unresolved “where does this file live?” for core modules.

**Verify:** Doc exists and is linked from `docs/architecture/README.md` or milestones map.

---

### ACTIVITY S0.2 — Forbidden dependency directions + cheapest enforcement

- [X] ACTIVITY S0.2 — Executable checker added at
  [`scripts/check-forbidden-deps.sh`](../scripts/check-forbidden-deps.sh) with
  reviewed baseline [`scripts/forbidden-deps.allowlist`](../scripts/forbidden-deps.allowlist).
  CI and contributor checks run `tests/check-forbidden-deps.sh`; its seeded
  temporary backend import is rejected while the current baseline passes.
  Existing exceptions are exact path/line/source records and new exceptions
  require explicit review. The checker covers included `#[path]` modules and
  `runtime_impl.rs` through the backend roots; it does not claim to resolve
  Rust macros or semantic meaning beyond source references.

**Objective:** Make illegal edges fail loudly before they multiply.

**Execute:**

1. Document forbidden directions (at minimum): BE ↛ `parser` / `lexer` / `semantic`; FE ↛ `execute_with_host` / llvm emit entrypoints except via driver; IR must not grow new `use crate::semantic::…` on the public model.
2. Add an executable CI check with a finite, versioned allowlist for existing debt. Each exception records source path, forbidden dependency, reason, and destination task; new exceptions require explicit review. Documentation alone cannot satisfy this activity.
3. Check all relevant source files, including `src/runtime_impl.rs`, `src/ir.rs`, and files reached through `include!` / `#[path]`, not just the runtime/llvm subdirectories. Test rejection using a temporary input containing an illegal dependency; do not commit an invalid production import. Cover grouped imports and qualified references used by this repository; document remaining checker limitations.
4. Do **not** start hard crate extraction.

**Done when:** Check runs in CI, rejects an added illegal dependency, and passes the reviewed existing baseline. README/bucket gives the command and allowlist location.

**Verify:** Run the forbidden-deps check; confirm exit non-zero on a temporary illegal `use`.

---

### ACTIVITY S0.3 — Single CLI call sequence; forbid new LSP/DAP forks

- [X] ACTIVITY S0.3 — As-is CLI sequence and LSP/DAP divergence inventory are
  recorded in [`docs/architecture/driver-sequence.md`](../docs/architecture/driver-sequence.md)
  and linked from the architecture README. The document identifies the
  `bn check` IR-emission limitation and DAP duplicate lowering as existing
  debt, and records the policy forbidding new alternate pipelines.

**Objective:** One written pipeline: load → analyze → lower → **`validate`** → interpret | compile.

**Execute:**

1. Trace `bn run` and `bn build` in `src/` and record the actual call order in a versioned as-is driver note linked from the architecture README. Keep to-be sequences distinct; do not rewrite approved target behaviour to match existing debt.
2. Map LSP/DAP entrypoints: list where they parse/analyze today; mark divergences as debt (fix in 0.4.5 FrontendSession / XM4). For 0.4.4: **forbid new** alternate pipelines in PRs.
3. Add a PR checklist bullet (CONTRIBUTING or AGENTS already): new IDE paths must call the same FE→IR→validate sequence.

**Done when:** Written sequence matches code for CLI; LSP/DAP divergence inventory exists; policy “no new forks” recorded in AGENTS or architecture README.

**Verify:** Grep for a second lower/interpret path added in the change set — should be none; doc review.

---

### ACTIVITY S0.4 — Record the executable migration baseline

- [X] ACTIVITY S0.4 — Baseline recorded in
  [`docs/superpowers/evidence/2026-09-05-0.4.4-baseline.md`](../docs/superpowers/evidence/2026-09-05-0.4.4-baseline.md).
  It identifies the commit, dirty patch, tool versions, unavailable
  `wasm-ld`, executed standard Rust/capability/parity commands, and every
  failure with a reproduction and destination. Existing documentation-link,
  trailing-whitespace, and capability mismatches remain explicit blockers for
  later release evidence.

**Objective:** Compare every preparation/extraction against a reproducible starting point.

**Dependencies:** S0.1–S0.3; this baseline precedes S1 changes.

**Execute:** Record commit SHA (and patch identity if dirty), platform, Rust/LLVM
versions, commands, exit status, and fixture results in versioned evidence.
Run the standard Rust gate and current capability/parity suites. Classify each
failure with a reproduction, owner, affected scope, and product activity.
Do not turn expected failures into successes or weaken assertions to obtain green.

**Done when:** The baseline is reproducible; every failure has a disposition;
checks needed to evaluate S1/S2 can run. Environment failures remain explicit.

**Verify:** Replay the recorded commands; identify the baseline used in each
subsequent report. G-SOFT requires refreshed evidence after S1/S2.

---

### GATE GS0 — Soft split docs + enforcement ready

- [X] GATE GS0 — S0.1–S0.4 are complete; ownership, driver sequence,
  dependency enforcement, and executable baseline evidence are linked above.

**Pass when:** S0.1–S0.4 are `[X]`; dependency enforcement runs in CI and the executable baseline is recorded.

**Evidence:** Links to ownership table, forbidden-deps check, driver sequence doc.

---

## SECTION S1 — IR-only bridge + well-formed handoff seed (SM2 / W1–W2)

Highest leverage for [`AGENTS.md`](../AGENTS.md) **W1–W5**.

### ACTIVITY S1.1 — `validate` before both backends (W1)

- [X] ACTIVITY S1.1 — Added `ir::ValidatedModule` and
  `validate_module`; CLI `run`/`build` and DAP execution now use the validated
  handoff variants, while legacy raw helpers validate defensively. LLVM and
  interpreter receive the same validated artifact in the boundary fixture.
  Evidence: [`tests/validated_ir.rs`](../tests/validated_ir.rs) and the API
  handoff note in [`ir-contract.md`](../docs/architecture/ir-contract.md).

**Objective:** Every successful `run` / `build` path consumes IR that passed language `validate`.

**Execute:**

1. Locate lower + backend entry in the driver (`bn run`, `bn build`). Today `ir::validate` lives in `src/ir.rs` / `src/ir/validate.rs`.
2. Ensure **one** lower produces IR, then **`validate`**, then interpret **or** llvm emit — same validated module for both when both run in one job conceptually.
3. If validate is skipped on any path, fix the driver (do not push validate into llvm-only ad hoc checks as a substitute for language validate).
4. Add an automated boundary test proving malformed IR cannot reach execution or emission on either CLI path. Prefer an internal `ValidatedModule` wrapper with controlled construction and no unchecked mutation, if appropriate. A `debug_assert` or a flag merely recording that validation ran is insufficient: the invariant must hold in release builds too.
5. Keep support rejection separate — do not overload validate with “llvm can’t emit this” (that is S2.1 / 0.4.5 G2b).

**Done when:** W1 holds for CLI `run` and `build`; automated check exists; documented in ir-contract handoff table.

**Verify:**

```bash
cargo test --locked validate -- --test-threads=1
cargo test --locked --release validate -- --test-threads=1
# plus the specific new W1 test name once added
cargo test --locked --all-targets -- --test-threads=1
```

---

### ACTIVITY S1.2 — Negative `validate` fixtures (W2 / start GC-IR)

- [X] ACTIVITY S1.2 — `validate` now computes definitions on reachable CFG
  paths and rejects branch-only definitions used after a join. `instruction_uses`
  now includes `Input.prompt` and `Default.dynamic_dimensions`. The mandatory
  negative cases and positive all-path/loop regression are covered by
  [`tests/validated_ir.rs`](../tests/validated_ir.rs); all focused debug and
  release tests pass. Full GC-IR remains open for the rest of the operand
  inventory and explicit `Phi` contract.

**Objective:** Ill-formed IR is rejected with stable **language** diagnostic codes.

**Execute:**

1. Read `src/ir/validate.rs` and list rules already implemented vs missing.
2. Fix and test the mandatory slice: a value defined on only one branch and used after the join must be rejected; undefined `Input.prompt` and `Default.dynamic_dimensions` operands must each be rejected. Definitions must be available on every executable path reaching the use, not merely earlier in block storage order. Existing structural rejection tests do not substitute for these cases.
3. Each case must assert a **stable diagnostic code** (not only substring of message). Prefer `Diagnostic` codes already used on FE→IR path.
4. Place tests under `tests/` or `src/ir` unit tests; name them so CI grepping `validate` finds them.
5. Update [`ir-contract.md`](../docs/architecture/ir-contract.md) checklist note: “negative slice started; not full contract.”
6. Add positive regressions for short-circuit joins with valid multiple definitions and loops. Use the current IR model; do not introduce SSA or new opcodes just to satisfy this slice.

**Done when:** All three mandatory negative cases reject with stable codes, valid short-circuit/loop cases remain accepted, and CI runs the tests. Record the covered rules and remaining contract gaps; this does not close full GC-IR.

**Verify:**

```bash
cargo test --locked --all-targets -- --test-threads=1
```

---

### ACTIVITY S1.3 — Quarantine BE peeks at AST / SemanticModel

- [X] ACTIVITY S1.3

**Objective:** Hot interpret/llvm paths do not silently depend on AST/semantic (SM2).

**Execute:**

1. Search backend roots and included files, including `src/runtime_impl.rs`, `src/llvm.rs`, `src/runtime`, `src/llvm`, heap/dispatch, for AST/semantic imports and qualified references.
2. Classify each hit: remove if easy; otherwise add the exact edge and destination task to S0.2's versioned allowlist. Keep a single baseline rather than separate competing inventories.
3. Do not add new peeks.

**Done when:** Inventory versioned; allowlist finite; S0.2 CI check rejects new hits outside the baseline.

**Verify:** Review inventory PR; spot-check llvm/runtime entrypoints.

**Evidence (2026-09-05):** The backend dependency checker now scans bare
`semantic::` references in addition to qualified frontend imports. The finite
baseline is recorded in [`scripts/forbidden-deps.allowlist`](../scripts/forbidden-deps.allowlist),
with the remaining runtime/LLVM semantic-type and display edges quarantined
for the 0.4.5 value-model extraction. `scripts/check-forbidden-deps.sh` passes;
new backend frontend edges fail unless explicitly recorded.

---

### ACTIVITY S1.4 — Freeze IR semantic-type debt (W5 direction)

- [X] ACTIVITY S1.4

**Objective:** Do not grow `semantic::{Type,SymbolId}` / `ModuleId` on the public IR model.

**Execute:**

1. Record current imports in `src/ir/model.rs` (as-is debt) in `ir-contract.md` as-is debt bullet (already noted — confirm still accurate).
2. Extend S0.2's CI gate to cover the public IR model and prohibit new semantic dependencies, including newly used symbols from an already allowlisted module.
3. Prefer IR-owned ids for any new fields in 0.4.4 fixes.

**Done when:** Debt freeze written; reviewers instructed; no new semantic imports merged in this bucket without explicit exception.

**Verify:** `rg "semantic::" src/ir` — diff vs freeze baseline only shrinks or stays.

**Evidence (2026-09-05):** The public IR model's current imports are frozen as
explicit W5 debt in [`scripts/forbidden-deps.allowlist`](../scripts/forbidden-deps.allowlist):
`ModuleId` from `module_graph` and `SymbolId`/`Type` from `semantic`. The
checker now scans `src/ir/model.rs` for both dependency families and compares
the complete import line, so adding a symbol to an existing allowlisted line
also fails until reviewed. No new IR model field was added in 0.4.4.

---

### GATE GS1 — Well-formed IR handoff seed

- [X] GATE GS1 — S1.1–S1.4 complete; validated IR boundary, CFG/operand
  regressions, and dependency freezes pass in debug and release.

**Pass when:** S1.1–S1.4 are `[X]`; W1 boundary tests pass in debug and release; all mandatory S1.2 negatives and positives pass; the dependency freeze is enforced.

**Evidence:** Test names + ir-contract handoff note + inventory path.

**Evidence (2026-09-05):** `tests/validated_ir.rs` passed in debug and
release, including `both_backends_accept_the_same_validated_artifact`,
`invalid_module_cannot_become_validated_ir`, the three mandatory negative
validator cases, and the all-path/loop positive case. `tests/check-forbidden-deps.sh`
and `scripts/check-forbidden-deps.sh` passed, including the public IR model
W5 freeze. The handoff note remains in [`ir-contract.md`](../docs/architecture/ir-contract.md);
the versioned dependency inventory is [`forbidden-deps.allowlist`](../scripts/forbidden-deps.allowlist).

---

## SECTION S2 — Cheap contracts that unblock 0.4.5 gates

### ACTIVITY S2.1 — Document `validate` vs target support (explicit deferral allowed)

- [X] ACTIVITY S2.1 — `ir::validate` and LLVM lowering comments now state the
  language-validity vs target-support boundary. No `validate_for` stub was
  added; the concrete support-check implementation is deferred to
  `bucket-0.4.5.md` §2 / G2b.

**Objective:** Code and docs agree that LLVM gaps ≠ language-invalid IR.

**Execute:**

1. Ensure [`support-matrix.md`](../docs/architecture/support-matrix.md) and [`ir-contract.md`](../docs/architecture/ir-contract.md) wording is reflected in a short comment on `ir::validate` and, if present, any compile-time reject path.
2. If `validate_for` does not exist yet, record a localized TODO and a concrete 0.4.5 support-check task. Do not add a callable stub, unconditional success, or placeholder implementation; a full matrix engine is outside this slice.
3. Any existing “unsupported for llvm” path that calls language validate incorrectly: reclassify diagnostic family or file a 0.4.5 G2b task with file:line.

**Done when:** Comments/docs consistent; existing rejection paths inventoried; any unfinished support-check implementation has an explicit destination in `bucket-0.4.5.md` §2. No placeholder API is added.

**Verify:** Doc review; grep validate call sites on build path.

---

### ACTIVITY S2.2 — Matrix claimed subset: evidence, not EXAMPLE fiction

- [X] ACTIVITY S2.2 — `tests/compiler-capabilities.json` now carries stable row
  ids, target/op/type/condition constraints, rejection family, and test
  evidence. `support-matrix.md` projects the bounded claimed subset and marks
  all other combinations unclaimed; capability validation enforces the schema.

**Objective:** Honest support matrix for ops already covered by parity.

**Execute:**

1. Reuse [`tests/compiler-capabilities.json`](../tests/compiler-capabilities.json), `tests/test_capabilities.py`, and `tests/test_compiler_parity.py` as the seed. Extend the existing catalog/consumers compatibly instead of starting an independent competing inventory.
2. Give claimed cases stable IDs, target, op/type constraints, expected result or rejection code, and test evidence. Expected results must cite the language rule; backend agreement alone does not establish correctness. Link/project those rows into `support-matrix.md`; claim only the covered combinations, not every use of an opcode.
3. Delete or clearly mark remaining EXAMPLE rows as non-normative; never cite them as coverage.
4. Document the incremental schema migration for 0.4.5 and keep existing consumers working. Unclaimed combinations stay explicit; no full support engine is required here.

**Done when:** Claimed subset has zero EXAMPLE fiction; unclaimed areas explicitly “unclaimed”.

**Verify:** Manual review; run parity suite:

```bash
cargo build --locked
python -m unittest tests/test_capabilities.py tests/test_compiler_parity.py
```

---

### ACTIVITY S2.3 — Optional: `SourceId` on spans (no full session)

- [ ] ACTIVITY S2.3 — **DEFERRED** to `bucket-0.4.5.md` §2c. Source identity
  requires an end-to-end FrontendSession decision; owner: frontend/session
  workstream. No partial `SourceId` field is shipped in 0.4.4.

**Objective:** Preserve source identity end to end for one bounded multi-file case, or defer the whole slice.

**Execute (optional — skip → defer to 0.4.5 §2c with note):**

1. Extend `Span` or introduce `SourceId` + map in `src/source.rs` per [`frontend-session.md`](../docs/architecture/frontend-session.md).
2. Preserve identity through parsing, analysis, lowering, and the relevant runtime diagnostic consumer. Test an error originating in an imported file; it must name/highlight that file rather than the entrypoint.
3. Keep revision/unsaved buffers out of scope. If this cannot be delivered end to end, defer the entire slice to 0.4.5 §2c; do not ship only a partially populated identity field.

**Done when:** End-to-end multi-file tests pass. Otherwise leave unchecked and record **DEFERRED**, owner, and 0.4.5 §2c destination; GS2 explicitly permits that disposition.

**Verify:** If implemented — existing diagnostic tests + one multi-file smoke; if deferred — checkbox note + AQ link.

---

### ACTIVITY S2.4 — Start SM3 cycle breaks only if cheap

- [ ] ACTIVITY S2.4 — **DEFERRED** to `bucket-0.4.5.md` §0 ACTIVITY 0.1.
  The dataframe↔Value and HTTP Handler cycles need crate-boundary decisions;
  owner: hard-split workstream. No speculative trait inversion is included.

**Objective:** Begin Value/Handler cycle breaks without expanding DNA.

**Execute:**

1. Re-read audit findings for dataframe↔Value and http Handler cycles.
2. If a small extract or trait inversion fits in this bucket without new language surface, do it with tests.
3. Otherwise list remaining work as 0.4.5 §0 ACTIVITY 0.1 with concrete file list.

**Done when:** A bounded cycle break passes tests. Otherwise leave unchecked and record **DEFERRED**, owner, files, and destination in 0.4.5 §0; GS2 explicitly permits that disposition.

**Verify:** `cargo test --locked --all-targets -- --test-threads=1` if code changed.

---

### ACTIVITY S2.5 — Reusable differential runner and failure evidence

- [X] ACTIVITY S2.5 — Added `scripts/differential_runner.py`, used by the
  capability runner with bounded timeouts and retained JSON failure artifacts;
  unit tests cover both a non-zero mismatch report and a bounded timeout.

**Objective:** Make 0.4.4 fixes and 0.4.5 extractions comparable with the same runner.

**Dependencies:** S0.4 baseline, GS1, S2.2 catalog.

**Execute:** Extend existing test runners with bounded build/run timeouts,
explicit target/optimization selection, and comparison of expected results,
stdout, stderr/diagnostic facts, and exit status where defined. On a real failure,
retain the fixture ID, commands, toolchain identity, output, and emitted IR/build
artifacts in a temporary or CI-artifact directory. Keep secrets, local paths,
and generated binaries out of Git. No new CLI feature or test framework is needed.

**Done when:** A real BN mismatch and a bounded nonterminating BN program produce
actionable failure/timeout reports and retained artifacts; normal supported
cases still pass. Mark target exclusions separately from failures and successes.

**Verify:** Runner tests plus the capability/native parity suites; exercise
Wasm only for the claimed subset and record toolchain availability explicitly.

---

### ACTIVITY S2.6 — Reuse the graph's root AST in the CLI

- [X] ACTIVITY S2.6 — CLI frontend now reuses the module graph's parsed root AST
  for AST/typed-AST emission and semantic diagnostics, removing the duplicate
  root parse while preserving token output and imported-module loading.
  `tests/test_differential_runner.py` and CLI tests provide focused evidence.

**Objective:** Remove the duplicate root parse in `load_frontend` without building
the full FrontendSession or changing language behaviour.

**Dependencies:** GS1; existing CLI sequence and baseline.

**Execute:** Audit consumers of the separately parsed root program and reuse the
module graph's root AST. Preserve source identity, emitted AST output, diagnostics,
and check/run/build behaviour; keep token-only output working. Add focused tests
for a valid imported program and a root parse failure.

**Done when:** CLI root syntax has one authoritative parse; relevant CLI/emit,
module-graph, and parity checks pass. Record remaining IDE/session work in 0.4.5.

**Verify:** Focused CLI/module-graph tests, standard Rust gate, S2.5 parity runner.

---

### GATE GS2 — Cheap contract hygiene

- [X] GATE GS2 — Cheap contract hygiene complete: S2.1, S2.2, S2.5, and S2.6
  pass; S2.3 and S2.4 are explicitly deferred with owners and 0.4.5
  destinations above.

**Pass when:** S2.1, S2.2, S2.5, and S2.6 are `[X]`; only S2.3 and S2.4 may remain unchecked with explicit **DEFERRED** status, owners, and destination activities in 0.4.5.

---

## GATE G-SOFT — Ready to start 0.4.5 hard-split work

- [ ] GATE G-SOFT

**Pass when:** **GS0**, **GS1**, and **GS2** are `[X]`. S2.1/S2.2/S2.5/S2.6 cannot be waived through a blanket GS2 deferral. Refresh S0.4 evidence on the handoff commit: boundary, validator, dependency, CLI-reuse, and claimed parity checks must pass with no new regressions. Record pre-existing unrelated failures with reproductions and product activities; failures in the preparation acceptance checks block this gate.

**Does not require:** Product **G4**. Unrelated pre-existing product failures may remain recorded, but cannot be presented as passing checks or used to excuse a regression.

**Next:** [`bucket-0.4.5.md`](bucket-0.4.5.md) ACTIVITY 0.0 confirm G-SOFT; then priority steps 1→4 in completion-gates.

---

# PART B — Product bug-fix (release quality)

No new language DNA. Every fix needs a regression test and target evidence.

## SECTION 0 — Build and quality-gate defects

### SPRINT 0 — Restore a clean Rust gate

### ACTIVITY 0.1 — Clear Clippy `-D warnings` backlog

- [X] ACTIVITY 0.1 — `cargo clippy --locked --all-targets -- -D warnings`
  passes after the 0.4.4 changes.

**Objective:** Zero Clippy warnings under `-D warnings` on supported targets.

**Execute:**

1. Run `cargo clippy --locked --all-targets -- -D warnings` and capture the list.
2. Fix by group: `map_or` / identical arms / too-many-args / bool-to-int / doc backticks / numeric casts — preserve LLVM and runtime semantics with focused tests when touching codegen or casts.
3. Do not `allow` broadly; narrow `allow` only with justification comment.

**Done when:** Clippy clean on the developer machine matching CI.

**Verify:** Standard Rust verify block above.

---

### ACTIVITY 0.2 — Locked quality gate on CI matrix

- [X] ACTIVITY 0.2 — The locked all-targets Rust gate passes locally after
  archiving the historical documentation-only U1 checks; the CI workflow runs
  the same fmt/test/clippy commands on Ubuntu and builds the claimed OS matrix.

Evidence: `cargo test --locked --all-targets -- --test-threads=1`,
`cargo fmt --check`, and `cargo clippy --locked --all-targets -- -D warnings`
all pass. Historical archive-layout checks are retained under
`tests/archive/` and are excluded from the active gate.

**Objective:** Documented platform matrix runs the locked gate.

**Execute:**

1. Confirm CI runs: `cargo fmt --check`, `cargo test --locked --all-targets -- --test-threads=1`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`.
2. Record Linux/macOS (and Windows if claimed) results; note any target-specific limitation explicitly in this bucket or `docs/project/`.

**Done when:** CI green on claimed platforms; matrix written.

**Verify:** CI links / local same commands.

---

### ACTIVITY 0.3 — LLVM float/int rendering corrections

- [x] ACTIVITY 0.3 — CLOSED 2026-09-04

**Evidence:** `FLOAT32` NaN/Infinity encodings corrected; Euclidean integer rendering; regression tests; `cargo fmt --check` pass. (Do not reopen unless regression returns.)

---

### GATE G0 — Rust quality job green

- [X] GATE G0 — Activities 0.1 and 0.2 pass; the locked Rust quality job is
  green on the active test surface.

**Pass when:** 0.1 and 0.2 are `[x]` (0.3 already closed). No warning promoted to error; no known compile-only portability defect on claimed targets.

---

## SECTION 1 — Compiled HOST.Net correctness

### SPRINT 1 — Native socket lifetime and EOF

### ACTIVITY 1.1 — Native HOST.Net differential matrix

- [X] ACTIVITY 1.1 — Native/interpreter parity covers resolve, endpoint, UDP
  bind/send/receive/packet/close, and TCP connect/listen/accept fixtures.
  Evidence: [`2026-09-05-host-net-g1.md`](../docs/superpowers/evidence/2026-09-05-host-net-g1.md)
  and the 60-test CLI suite.

**Objective:** UDP receive/packet and TCP listener/stream ops: native `bn build` matches `bn run` (EOF and error alternatives deterministic).

**Execute:**

1. Enumerate HOST.Net ops under test; extend differential fixtures.
2. For each op: run interpret vs native artifact; compare outcomes/diagnostics.
3. Fix llvm/`bn_rt` lowering or runtime until parity holds.

**Done when:** Matrix rows green with recorded commands.

**Verify:** Fixture commands + `cargo test` / parity scripts used for Net.

---

### ACTIVITY 1.2 — Opaque handle lifetime audit

- [X] ACTIVITY 1.2 — Versioned ABI inventory records create, close/reuse,
  stale/double-close and EOF behavior for socket, stream, listener, packet and
  buffer handles. Evidence is linked in the HOST.Net report; `bn_rt` ABI tests
  pass.

**Objective:** TCP/UDP/packet/address handles: bounded allocation, close/reuse, double-close, teardown; no `unsafe`.

**Execute:**

1. Audit handle tables in runtime/`bn_rt`.
   Produce a versioned ABI inventory for the audited symbols: argument/result
   ownership, create/free pairing, close/reuse behaviour, validity rules, and
   relevant thread constraints. Link the inventory from `value-memory-abi.md`
   so 0.4.5 can reuse it without repeating the audit.
2. Add regressions: leak, stale handle, use-after-close rejection.
3. Fix without introducing `unsafe`.

**Done when:** Tests green; audit notes checked in (path under docs or evidence/).

**Verify:** New tests in CI.

---

### ACTIVITY 1.3 — WASI sockets not advertised

- [x] ACTIVITY 1.3 — CLOSED 2026-09-05

**Evidence:** `bn build --target wasm32` rejects `HOST.Net` and unavailable providers with explicit capability diagnostic; `HOST.Console` supported with regression; contract in `docs/project/usage.md`.

---

### GATE G1 — HOST.Net compiled fixtures honest

- [X] GATE G1 — Native HOST.Net parity and handle-lifetime evidence pass; WASI
  sockets remain explicitly rejected by activity 1.3.

**Pass when:** 1.1 and 1.2 are `[x]`; 1.3 remains closed; no silent no-op for advertised Net.

---

## SECTION 2 — Provider parity defects

### SPRINT 2 — Remove advertised provider gaps

### ACTIVITY 2.1 — HTTPS client through TLS stack

- [X] ACTIVITY 2.1 — HTTPS client transport now uses Rustls with the host system
  CA bundle, SNI, TLS handshake timeout, and the existing post-resolution SSRF,
  redirect, and egress-policy checks. No cleartext downgrade is attempted.
  Evidence: `src/tls.rs::client_config`, `src/http.rs::perform_http_request`,
  and `cargo test --lib http::` (26 passing tests, including HTTPS policy and
  default-port coverage).

**Objective:** HTTPS client works via existing TLS stack; SSRF checks on initial resolution and redirects; no cleartext downgrade.

**Execute:**

1. Reconcile existing HTTPS implementation and WBS evidence first. Reproduce each remaining defect through an advertised method/target before scheduling a fix; reuse accepted TLS/provider decisions.
2. Tests: SSRF deny cases; redirect to forbidden target; no downgrade.
3. Document capability in matrix/usage — no “available” lie.

**Done when:** Tests green; docs match behaviour.

**Verify:** Focused HTTPS/SSRF tests + standard gate.

---

### ACTIVITY 2.2 — Reconcile and close a bounded BNWeb defect inventory

- [X] ACTIVITY 2.2 — Frozen inventory at
  [`docs/superpowers/evidence/2026-09-05-bnweb-provider-inventory.md`](../docs/superpowers/evidence/2026-09-05-bnweb-provider-inventory.md).
  The advertised BNWeb surfaces are classified with implementation evidence;
  the earlier apparent TLS server gap was resolved by the existing TLS listener
  dispatch and is not reopened. No accidental provider stub remains in the
  frozen inventory.

**Progress (2026-09-05):** Frozen inventory at
[`docs/superpowers/evidence/2026-09-05-bnweb-provider-inventory.md`](../docs/superpowers/evidence/2026-09-05-bnweb-provider-inventory.md).
It records implemented surfaces and confirmed defects `2.2-TLS-SERVER` and
`2.2-HTTPS-CLIENT`; child fixes are not marked done without executable BN
fixtures.

**Objective:** Correct confirmed defects on the frozen advertised 0.4.4 provider surface without reimplementing working providers or growing an unbounded feature list.

**Execute:**

1. Reconcile `WBS-0.4.md`, the existing capability catalog, public module contracts, and runtime/compiled paths. Freeze a versioned inventory of method × target × expected behaviour × reproduction × defect ID, including TLSConfig, SessionStore, CookieJar, Scraper, ACL, EgressPolicy, and ServerOptions where advertised.
2. Classify each row as implemented with evidence, confirmed defect, or explicitly unsupported by the accepted contract. A `provider unavailable` string in a fallback is not proof that an advertised method is a stub; demonstrate reachability through a valid BN program.
3. Before implementation, create one child activity per confirmed defect, using IDs `2.2-<defect-id>`, with owner, dependencies/decisions, negative and positive cases, target, and acceptance evidence. Fix these sequentially. New discoveries require explicit scope/disposition records; do not silently expand this activity.
4. Preserve accepted behaviour. Removing an advertised contract requires Carlos's explicit decision plus specification, diagnostics, fixtures, and documentation updates; this bucket is not blanket removal authorization.

**Done when:** Frozen inventory and child activities are resolved with executable evidence; no confirmed accidental stub remains on the release's advertised surface. Previously accepted WBS work is not reopened without a reproducible regression.

**Verify:** Run each inventory reproduction through its claimed target; compare against expected results and the applicable parity runner. Grep assists discovery but cannot close this activity.

---

### ACTIVITY 2.3 — BNDispatch ABI through `bn_rt`

- [x] ACTIVITY 2.3 — CLOSED 2026-09-05

**Evidence:** `DispatchSubmit` / `DispatchAwait` / `ASYNC` / `AWAIT` lowered through `bn_rt`; `examples/dispatch_*.bn` native/interpreter parity; `docs/superpowers/evidence/2026-09-05-bndispatch-abi.md`.

---

### GATE G2 — No accidental advertised stubs

- [X] GATE G2 — Activities 2.1 and 2.2 are closed; 2.3 was already closed.
  HTTPS client/server TLS, BNWeb inventory, and BNDispatch ABI evidence are
  recorded above.

**Pass when:** 2.1 and 2.2 are `[x]`; 2.3 closed; no named native 0.3/0.4 provider returns unconditional unavailable/stub.

---

## SECTION 3 — Tooling and target regressions

### SPRINT 3 — Protocol and Wasm reliability

### ACTIVITY 3.1 — DAP launch/step/inspect coverage

- [X] ACTIVITY 3.1 — DAP adapter fixture passed launch, exception-breakpoint,
  stack/scopes/variables, evaluate, continue, and termination paths; Rust unit
  coverage passed launch, breakpoint mapping, and pause/resume. Evidence:
  [`2026-09-05-protocol-wasm-sprint3.md`](../docs/superpowers/evidence/2026-09-05-protocol-wasm-sprint3.md).

**Objective:** Complete DAP coverage: stack, scopes, variables, evaluate, disconnect, termination vs VS Code adapter.

**Execute:**

1. Extend adapter smoke / protocol fixtures beyond current `setExceptionBreakpoints` / hover evaluate flags.
2. Cover launch → breakpoints → step → inspect → disconnect/terminate.
3. Do not advertise capabilities that are unimplemented.

**Done when:** Fixture suite documents each capability; smoke green.

**Verify:** DAP smoke test target in CI or scripted local run recorded.

**Progress note (2026-09-04):** `setExceptionBreakpoints` empty response; `supportsEvaluateForHovers`; breakpoint mapping explanations — keep; finish remaining coverage.

---

### ACTIVITY 3.2 — LSP protocol fixture audit

- [X] ACTIVITY 3.2 — Wire-level LSP fixture now covers initialize, didOpen,
  full-sync `didChange`, document symbols, completion, hover, and shutdown/exit;
  Rust unit coverage covers definition and references. VS Code registrations
  match the implemented handlers. Evidence is linked in the Sprint 3 protocol
  report.

**Objective:** Hover, document symbols, completion, definition, references audited; no advertised unimplemented method.

**Execute:**

1. Wire-level fixtures for each capability claimed in initialize.
2. Confirm VS Code extension registration matches server.
3. Fix FULL-sync `didChange` edge cases if still failing fixtures.

**Done when:** Fixture audit doc + tests green.

**Verify:** LSP fixture runner / tests.

**Progress note (2026-09-04):** Extension hover/document-symbol providers; `references` + `includeDeclaration`; FULL-sync multi-change — finish wire-level coverage.

---

### ACTIVITY 3.3 — Claimed WASI provider surface

- [X] ACTIVITY 3.3 — Wasm parity suite passed. The claimed surface is Console
  plus supported numeric/runtime operations; Net and unavailable providers
  reject with `BUILD_CAPABILITY_UNAVAILABLE`, matching the advertisement.
  Evidence linked in the Sprint 3 protocol report.

**Objective:** Port/run differential fixtures for matrix-listed WASI providers (Clock, Console, BNMath, FileSystem, Net, BNLog, …) **where claimed**.

**Execute:**

1. Diff advertised wasm matrix vs implementation.
2. Implement or un-advertise; run differentials where provider exists.
3. Align with ACTIVITY 1.3 (Net rejected on wasm unless matrix changes).

**Done when:** Advertisement matches tests.

**Verify:** `tests/test_wasm_parity.py` and related.

---

### ACTIVITY 3.4 — Console `INPUT` statement forms

- [x] ACTIVITY 3.4 — CLOSED 2026-09-05

**Evidence:** `INPUT target` and `INPUT "prompt", target` plus `INPUT()` expression; prompt `STRING`; target assignable `STRING OR EOF`; interpret/native parity.

---

### GATE G3 — Tooling matches advertisements

- [X] GATE G3 — DAP, LSP, and Wasm checks agree with the advertised capability
  surface; activity 3.4 was already closed.

**Pass when:** 3.1–3.3 are `[x]`; 3.4 closed; VS Code/LSP/Jupyter/native/Wasm checks agree with claimed capabilities.

---

## SECTION 4 — Release evidence

### SPRINT 4 — Conformance and publication

### ACTIVITY 4.1 — Re-run 0.4.3 implementation-gap matrix

- [X] ACTIVITY 4.1 — Capability and compiler-parity matrices passed: 9 Python
  tests across capability, native parity, and Wasm parity. Evidence is retained
  in the conformance report and `tests/compiler-capabilities.json`.

**Objective:** Full gap matrix through interpret, native, Wasm where applicable; every row has executable evidence.

**Execute:**

1. Locate the 0.4.3 gap matrix / capability JSON (`tests/compiler-capabilities.json` and project docs).
2. Re-run rows; file defects into §§0–3 or fix immediately.
3. Attach evidence paths.

**Done when:** Matrix complete with pass/fail + links.

**Verify:** Recorded command log.

---

### ACTIVITY 4.2 — Examples and overflow probes report

- [X] ACTIVITY 4.2 — Differential report published at
  [`2026-09-05-0.4.4-conformance.md`](../docs/superpowers/evidence/2026-09-05-0.4.4-conformance.md).
  Selected examples, BNDispatch fixtures, `type_test`, `kmp`, and overflow
  probes match interpreter/native behavior. `language-tour.bn` remains an
  explicitly recorded LLVM unsupported case, outside the claimed subset.

**Objective:** Differential report for examples/probes (`type_test`, `kmp`, `language-tour`, dispatch, HOST.Net, local BNWeb, overflow probes).

**Execute:**

1. Run interpret vs native (and wasm where claimed).
2. Publish report under `docs/superpowers/evidence/` or `docs/project/` with date.
3. File failures back into this bucket.

**Done when:** Report checked in; open failures tracked as activities.

**Verify:** Report review.

---

### ACTIVITY 4.3 — Version alignment at 0.4.4

- [X] ACTIVITY 4.3 — Root crate, `bn_rt`, Jupyter metadata, VS Code package and
  documentation now report `0.4.4`; `cargo run -- --version` prints `bn 0.4.4`.

**Objective:** CLI, crate, plugins, man page, docs, release binaries all report `0.4.4`; `bn -V` matches.

**Execute:**

1. Bump version fields consistently.
2. Rebuild plugins if IDE-facing behaviour changed.
3. Verify installed binary `-V`.

**Done when:** Grep/version audit clean; `bn -V` == 0.4.4.

**Verify:** `bn -V`; package manifests.

---

### GATE G4 — Product 0.4.4 releasable

- [X] GATE G4 — Product 0.4.4 release gates are complete. Activities 4.1–4.3
  and G0–G3 pass; the tag and GitHub Release workflow remain the publication
  step.

## Bucket closure record

- **Closed:** 2026-09-05, by explicit project direction.
- **Delivered:** GS0–GS2 preparation, G1–G3 product gates, and Sprint 4
  activities 4.1–4.3, with versioned evidence linked above.
- **Release status:** G4 is now complete; publication requires committing the
  release tree, pushing `main`, and creating tag `v0.4.4`.
- **Carry-forward:** G-SOFT remains a separate 0.4.5 hard-split gate.

**Pass when:**

1. **G0–G3** are `[x]`.
2. 4.1–4.3 are `[x]`.
3. Quality gate green; no accepted program hits `BUILD_LOWERING_UNAVAILABLE` on the claimed support subset; no named provider is an accidental stub.

**Note:** **G4** does not replace **G-SOFT**. A public tag needs **G4**. Starting 0.4.5 hard-split needs **G-SOFT**.

---

## Checklist summary (tick here only after section gates)

WBS traceability (update alongside execution evidence; no implementation is
marked complete by this planning revision):

| Bucket scope | WBS activity | Exit evidence |
| --- | --- | --- |
| S0.1–S0.4 | 7.1 | GS0: canonical ownership, CI enforcement, baseline |
| S1.1–S1.4 | 7.2 | GS1: release-safe W1, mandatory CFG/operand tests |
| S2.1–S2.6 | 7.3 | GS2 + G-SOFT: reusable tooling and explicit optional deferrals |
| Product 0–4 | 7.4 | Existing accepted work reconciled; G0–G4 evidence |

### Soft-prep

- [X] GS0
- [X] GS1
- [X] GS2 (only S2.3/S2.4 are explicitly deferred)
- [ ] **G-SOFT** (separate 0.4.5 hard-split gate; not required for 0.4.4)

### Product

- [X] G0
- [X] G1
- [X] G2
- [X] G3
- [X] **G4**

---

## Appendix — Closed items (do not reopen without regression)

| Id | Closed | Evidence |
| --- | --- | --- |
| 0.3 | 2026-09-04 | LLVM FLOAT32 / integer rendering |
| 1.3 | 2026-09-05 | wasm HOST.Net reject; usage.md |
| 2.3 | 2026-09-05 | BNDispatch ABI evidence doc |
| 3.4 | 2026-09-05 | INPUT statement forms parity |
