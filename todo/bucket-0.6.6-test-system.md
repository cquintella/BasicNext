# Bucket 0.6.6 — Test system: speed, visibility, selection, verification

**Status:** Planned (todo). Not started. Was 0.6.7 until 2026-10-14, when
Carlos moved it ahead of BNFfi (now `todo/bucket-0.6.7.md`), so that BNFfi
is written in the new test layout and verified by sections V and M.
Updated 2026-10-18: structural bin consolidation moved early to eliminate
root clutter and the macOS scan stall; point-to-point verification and
comprehensive audit of existing tests integrated.
**Starts after:** `ongoing/bucket-0.6.5.md` closes (the M.1 ledger is
frozen on a suite that no longer changes under the 0.6.5 corrections).
**Tag:** `docs/releases/0.6.6.md` on close (test system; no language
change).

**Claim:** a faster test suite and inner loop (less build, I/O and process
contention), eliminating the 31 loose `.rs` test binaries from `tests/`,
with every test visible, selectable, audited and timed; keeping full
coverage and strict equivalence against the current harness via point-to-point
verification; and a suite whose expected results come from the specification,
not from the toolchain, running against real components, fuzzing inputs and
exploring concurrency where the risk is (section V).
Every test migrates with its quality and coverage proved, domain
by domain, by the migration protocol (section M).

Property-based and mutation testing beyond the migration verifier are not in
this bucket: they are `todo/proposals/property-and-mutation-testing.md`.

**Method:** sprints ordered so that mechanical cleanups with immediate
compilation/linking gains come first, followed by audit and harness integration,
culminating in feature-based layout and verification. Every new technique runs
alongside the old one; it replaces it only with point-to-point measured proof
and 100% of the old coverage.

---

## Invariants

- **Comparative execution:** no current test or harness is retired before
  its replacement runs alongside it and produces the same (or stronger)
  results. Section M is the method that proves it; "same results" without
  M's point-to-point evidence is not a migration.
- **Point-to-point verification:** every moved or consolidated test is verified
  individually: identity in `cargo test -- --list`, equivalence of assertion
  profiles (M.2), identical or strictly tighter stdout/stderr checks, and
  differential branch/region coverage (M.3).
- **Semantics over speed:** no harness optimization changes error-assertion
  semantics or the W1–W5 contracts.
- **Root cleanliness and domain cohesion:** no loose integration test `.rs` files
  remain in the root of `tests/`. Tests are grouped into cohesive domain test suites
  or declarative feature folders (`tests/features/`).
- **Independent oracle:** an expected result is derived from the
  specification (`language/0.6/` and companion contracts) or computed by hand,
  never recorded by running `bni` or `bnc`. `bni` and `bnc` share frontend,
  lowering and IR, so agreement between them proves nothing about a defect
  upstream of the backends.
- **No mocks:** a test of a component that talks to SQLite, the file
  system, the network, a child process, clang or node uses the real one.
- **Measured, not estimated:** every claimed gain cites before/after numbers
  and the machine.

---

## Facts (2026-10-08, updated 2026-10-18; macOS arm64 dev machine)

| Fact | Evidence |
| --- | --- |
| Root clutter: 31 separate `.rs` test files in `tests/` root, each compiled as an independent Cargo test binary with full link overhead | `ls tests/*.rs \| wc -l` |
| `scripts/test-battery.sh` records per-binary wall time in `target/test-timings.txt` and orders longest-first; `JOBS=1` due to macOS scan stall | the script |
| Last battery: 64 binaries, 1 338 s; **42 binaries take 15–17 s each (582 s in total)**, including trivial ones (`lexer`, `bn_value`) | `target/test-timings.txt` |
| That floor is first-exec wait from macOS AMFI/syspolicyd, not test work: `lexer` test binary, first run `real 13.24 s`, `user 0.02 s`; second run `real 0.02 s` | `/usr/bin/time -p` |
| Built executables carry `com.apple.provenance`, not `com.apple.quarantine` | `xattr target/debug/bni` |
| Compilation profile optimizations (`split-debuginfo = "unpacked"`, `debug = 1`, deps `opt-level = 2`) active in `Cargo.toml` | `Cargo.toml` |
| `tests/runtime.rs` runs programs in process (`HostEnv`, 2 subprocess calls in 208 tests); `tests/semantic.rs` has none; `tests/parity.rs` has 103 | `grep -c 'Command::new\|bni()\|bnc()'` |
| Fragile assertions: 54 substring `.contains(` assertions (`parity.rs` 29, `runtime.rs` 13, `semantic.rs` 6, `parity_regressions.rs` 6) | `grep -c '\.contains('` |
| Stale binary vulnerability: root `Cargo.toml` dev-dependencies do not track `bni`, `bnc` or `libbn_rt.a` rebuilds when running focused integration tests | `Cargo.toml` |
| 55 resolved K# (`done/known_issues-0.6.5-resolved.md`), 30 open (`known_issues.md`); only 2 commits (K68, K6) are small, isolated and revertible alone | `grep -c '^### K'`, `git log --oneline` |

---

## Test execution pipeline (design; Carlos, 2026-10-10)

The pipeline every test run follows, from a change to its evidence. Each
stage has an input, an output written to a file, a feedback line, and the
adjustment its result triggers.

### Three depths

| Depth | When | Stages |
| --- | --- | --- |
| Inner loop | while editing | P0 (focused scope), P3, P4 for the touched crates/suites |
| Item gate | before marking a bucket item `[X]` | P0–P7 on touched crates and interfaces |
| Close gate | a sprint or bucket closes | P0–P9 in full, every platform, plus V.3 fuzzing budget and V.4 concurrency jobs |

### Stages

| Stage | Input | Output (under `target/pipeline/<run>/`) | Feedback | Adjustment |
| --- | --- | --- | --- | --- |
| **P0 Scope** | the change (`git diff` against `HEAD` or bucket item) | `scope.json`: touched crates, spec files, test selection with `covers:` tags | one line: depth, tests selected, why | shared core, IR, spec or harness change widens scope to full suite |
| **P1 Snapshot** | working tree as Git sees it (`rsync` copy) | `snapshot.txt`: source commit, hash of copied tree | snapshot id printed at start/end | lock file prevents overlapping gates; edits during run don't pollute gate |
| **P2 Static gates** | the snapshot | logs: `fmt`, `git diff --check`, `clippy -D warnings`, `source_size`, `check-shared-cores.sh`, text literals | `static: fmt=ok clippy=FAIL(3)...` | red stops run: fix code, never raise ceiling or add allow-list |
| **P3 Build** | the snapshot | build log, time, hash of `bni`, `bnc`, `libbn_rt.a`; automated stale-guard | `build: 22 s` or compile error | stale-guard rebuilds `bni`/`bnc`/`libbn_rt.a` if sources changed |
| **P4 Unit** | core and crate unit tests (`crates/*/src`) | JUnit with per-test duration | `unit: 1 210 passed, 0 failed` | failure classified (P9) before continuing |
| **P5 Conformance** | spec fixtures (`tests/grammar`, `semantic`, `validated_ir`, `ir`) | verdict, diagnostic code, W1–W5 / `GC-*` tag per fixture | spec section failed + diagnostic | fixture disagreeing with spec is toolchain defect |
| **P6 Cross-backend** | `tests/features/`, `host_conformance`, `alternatives_matrix`, `ownership_matrix` | `matrix.json`: case × backend (`bni`, `bnc`). Wasm withdrawn 2026-10-20 | table of red cells only | ratchet: passing known-failures removed; new red cell is defect |
| **P7 Integration** | loopback network, `HOST.Exec`, servers, child processes | JUnit; resource groups (ports, clang, temp dirs) | red tests + resource group | `nextest` test-groups serialize shared ports; flaky runs reported |
| **P8 Platforms** | snapshot on Linux, macOS, Windows (CI / Windows host) | per-platform summary + logs | `linux=ok macos=ok windows=FAIL` | platform defect reproduced on that target before fix |
| **P9 Classification** | outputs of P2–P8, duration history | `summary.txt`, `durations.csv`, `flaky.txt` | summary pasted into bucket item | classified as: product defect (K#), test defect, environment, or flaky |

---

## SPRINT 0 — Prerequisites, Tooling and Stale-Binary Guard

What must exist before any test moves: baseline tooling, ledger foundation,
stale-binary protection and the migration pilot.

- [ ] **S0.a** Install `cargo-nextest`, `cargo-llvm-cov`, `cargo-mutants`. Verify
  on macOS arm64, Linux, and Windows test host. Record versions.
- [ ] **S0.b** (Stale-Binary Guard) Implement in `tests/support/mod.rs` (or `build.rs`
  hook) an automated check: when an integration test invokes `bni`, `bnc` or
  links `libbn_rt.a`, verify timestamps against crate sources in `crates/`.
  If stale, automatically trigger `cargo build -p bni -p bnc -p bn_rt` once
  via a thread-safe `OnceLock` instead of relying on manual reminders.
- [ ] **S0.c** Freeze `tests/migration-ledger.csv` (M.1) from `cargo test --workspace -- --list`
  and fixture list. Add ratchet gate in `tests/` ensuring no test is dropped.
- [ ] **S0.d** Pilot domain: **K68** (comparison and `PRINT` of `STRUCT`, vector and
  object values) through M.1–M.5 end to end. Fallback: **K6** (interface reference).
  Verify that the pilot catches the defect when replayed (M.4).
- [ ] **S0.e** Document rule in `AGENTS.md` ("Changes and checks"): one K# per commit,
  with production change separable from its test.

---

## SPRINT 1 — Baseline, Snapshot Gate and Workspace Cleanliness

Isolate gates so editing during test runs does not corrupt evidence, taking
advantage of the shared `sccache` and APFS cloning on macOS.

- [ ] **S1.a** Record baseline: full run of `scripts/test-battery.sh` (wall time per binary,
  total time, peak memory) and `cargo test --workspace --no-run` link time.
- [ ] **S1.b** `scripts/gate-snapshot.sh`:
  - Snapshot the working tree into `bn/basicnext-gate/` using `rsync -a --delete`
    (properly deleting files removed in the working tree, excluding `.git/` and `target/`).
    On macOS APFS, leverage clonefile/copy-on-write.
  - Maintain an independent `target/` in `basicnext-gate/` to eliminate Cargo lock contention,
    while sharing object cache via the already configured `sccache`.
  - Enforce single-gate execution via a PID lock file (`target/gate.lock`).
  - Run `cargo fmt --check`, `git diff --check`, static gates, unit tests and full suite,
    outputting a single summary line (`fmt= diffcheck= build= clippy= test= tree=`).
- [ ] **S1.c** Verify snapshot isolation: make an edit in `basicnext/` while the gate
  runs in `basicnext-gate/`; confirm the gate result and working tree check remain clean.

---

## SPRINT 2 — Eliminate Root Test Clutter: Consolidate Loose Integration Binaries

Directly solve the 31 loose `.rs` files in `tests/` root and the 582s macOS first-exec stall.
Move loose integration test files into cohesive, domain-scoped integration suite binaries.

### Target Layout for `tests/`

Consolidate 31 root test files into 4 domain binaries plus shared support:

- `tests/frontend_suite/main.rs`: `mod lexer; mod parser; mod semantic; mod ir; mod validated_ir; mod ir_names; mod destructuring; mod u1;`
- `tests/compiler_suite/main.rs`: `mod codegen_tests; mod parity; mod parity_regressions; mod compiler_parity; mod compiler_capabilities; mod alternatives_matrix; mod ownership_matrix; mod value_matrix;` (`wasm_parity` withdrawn with the target, 2026-10-20).
- `tests/runtime_suite/main.rs`: `mod runtime; mod heap; mod host_conformance; mod arc_differential; mod abi_exports; mod module_graph;`
- `tests/gates_suite/main.rs`: `mod source_size; mod text_literals; mod dependency_gate; mod shared_cores; mod shared_logic; mod catalog_keys; mod suppressions;`
- Clean root: only domain runner folders (`tests/*_suite/`), `tests/features/`, `tests/support/`, `tests/fixtures/`, and gate shell scripts remain. Zero loose test `.rs` files in `tests/` root.

### Point-to-Point Verification Protocol for Consolidation

- [ ] **S2.a** Inventory: run `cargo test --test <old_binary> -- --list` for all 31 binaries
  and record the exact test list into `target/pre-consolidation-tests.txt`.
- [ ] **S2.b** Incremental migration: move test files into domain suites without altering
  internal test code, replacing individual root Cargo targets with the 4 domain runners.
- [ ] **S2.c** Point-to-point list diff: run `cargo test --test <domain_suite> -- --list`
  and assert that 100% of the previous tests exist with identical names under their module prefix.
- [ ] **S2.d** Comparative run: execute both old (from git stash/branch) and new suites;
  verify identical pass/fail status and assertion counts.
- [ ] **S2.e** Measure gains: record reduction in link time and macOS first-exec stall.
  Target: eliminate ~500s of macOS AMFI pause by dropping from 31 integration binaries to 4.

---

## SPRINT 3 — Comprehensive Audit and Review of Existing Tests

Thorough review of the existing test corpus to eliminate fragility, tighten assertions,
and identify gaps before declarative migration.

### Review Checklist and Categories

- [ ] **S3.a (Audit of Substring Assertions)**: Audit all 54 occurrences of `.contains(`
  in `parity.rs`, `runtime.rs`, `semantic.rs`, `parity_regressions.rs`:
  - Identify cases where `.contains("some substring")` was used as a shortcut for full output.
  - Tighten each to exact stdout comparison (`.out` files) or exact diagnostic code + line (`.err`).
  - Flag any assertion that tolerated extra garbage output or unhandled errors.
- [ ] **S3.b (Audit of Accidental Oracles)**: Audit tests where expected output was recorded
  by copying output of `bni`/`bnc` rather than deriving from `language/0.6/`:
  - Review numeric edge cases (overflow -> `Error`, float `PRINT`, integer bounds).
  - Review string operations (UTF-8 slicing, concatenation) and temporal operations.
  - Tag cases lacking explicit spec anchor with `oracle: bni-observed` in ledger.
- [ ] **S3.c (Audit of Concurrency and Shared Resources)**:
  - Identify tests sharing TCP loopback ports, child process handles, or temp files.
  - Map port allocations in `BNWeb` and `HOST.Net` tests to isolate port ranges or use
    dynamic port binding (`port 0`) to prevent flakiness under parallel execution.
- [ ] **S3.d (Audit of Dead and Redundant Tests)**:
  - Detect duplicate tests exercising identical AST/IR paths with no assertion difference.
  - Cross-check against closed K# issues to ensure regression tests accurately target
    the fixed defect rather than generic happy paths.
- [ ] **S3.e**: Publish findings in `tests/audit-report-0.6.6.md` with action items
  mapped to Sprints 4, 5 and Section V.

---

## SPRINT 4 — `cargo-nextest` Integration and Resource Groups

Integrate `cargo-nextest` as the primary local test execution engine alongside the battery.

- [ ] **S4.a** Configure `.config/nextest.toml`:
  - Enable JUnit reporting (`target/nextest-reports/junit.xml`) with per-test duration.
  - Configure `test-groups` to serialize tests competing for ports (BNWeb, HOST.Net)
    or heavy LLVM/clang invocations:
    `[test-groups.network] max-threads = 1`
    `[test-groups.compiler_heavy] max-threads = 2`
- [ ] **S4.b** Run `cargo nextest run` alongside `scripts/test-battery.sh`:
  - Verify 100% parity of test outcomes across the 4 consolidated suites and crate units.
  - Measure per-test execution speed and identify the slowest 5% of tests.
- [ ] **S4.c** Flaky test detection: configure `retries = 1` with a strict reporting policy:
  any test that fails on attempt 1 and passes on attempt 2 is recorded in `flaky.txt`
  and treated as a defect to be fixed, never hidden as green.

---

## SPRINT 5 — Declarative Feature Suite (`tests/features/`)

Transform test cases into declarative data files with individual nextest visibility.

### Layout and Architecture

`tests/features/<feature>/<case>.bn` accompanied by:
- `<case>.out`: expected stdout for positive execution.
- `<case>.err`: expected diagnostic code, line, and message identifier for negative checks.
- `<case>.exit`: expected exit code (if non-zero).
- `README.md`: normative spec reference (`language/0.6/0.6.md#...`) and implementation sites.

### Technical Implementation

- [ ] **S5.a (Individual Test Generation)**: Implement the declarative runner
  (via `build.rs` code generation or procedural macro) so that every discovered `<case>.bn`
  is emitted as an individual `#[test]` function in Rust (e.g. `features::vectors::bounds_check`).
  This ensures `cargo nextest` schedules, isolates, and measures each case individually.
- [ ] **S5.b (In-Process Fast Path for Negatives)**:
  - Tests expecting compilation errors (`.err`) run **in-process** via direct library calls
    to `bn_frontend::prepare` and `bn_ir::validate`.
  - Zero subprocess spawning and zero `clang` invocations for negative tests, executing
    in microseconds.
- [ ] **S5.c (Batch/Parity Execution for Positives)**:
  - Positive tests execute on `bni` (in-process or subprocess) and compile through `bnc`.
  - Compare stdout/exit code byte-for-byte with `.out` and `.exit` files.
- [ ] **S5.d (Point-to-Point Migration of Parity Fixtures)**:
  - Move tests from `tests/grammar/` and `parity.rs` domain by domain according to Section M.
  - `examples/` (including design patterns) validated through the same runner.
- [ ] **S5.e**: Gate: every case has matching expected-result files; every feature folder
  has a `README.md` with a valid spec anchor.

---

## SPRINT 6 — Test Index, Doc Tags and Selective Execution

Make every test queryable and selectable based on normative coverage.

- [ ] **S6.a** Metadata convention:
  - For `.bn` feature cases: header comment `' covers: language/0.6/0.6.md#section, GC-*`.
  - For Rust tests: doc-comment `/// covers: spec.md#section, W1-W5`.
- [ ] **S6.b** Static validation gate:
  - Ratchet gate in `tests/gates_suite/`: scans all `#[test]` and `.bn` files.
  - Rejects any test lacking a `covers:` tag, starting with the baseline shrink-only allow-list.
- [ ] **S6.c** Filtersets for `nextest`: define selection aliases in `.config/nextest.toml`
  allowing targeted execution by spec domain (`nextest run -E 'test(=features::vectors)'`).
- [ ] **S6.d (Test-Services Program)**: Parallel tool in Basic Next (`bn/basic-next-apps`)
  consuming JUnit XML and nextest JSON list for trend analysis and duplicate detection.

---

## SPRINT 7 — Targeted Harness Optimizations

Driven by the measurements collected in Sprints 2, 4 and 5.

- [ ] **S7.a** Dual-mode execution for `bni` tests: where CLI surface is not under test,
  run via in-process `HostEnv` with stdout capture, preserving subprocess tests only for
  `cli_snapshots.rs` and CLI argument verification.
- [ ] **S7.b** In-memory temp files (`TMPDIR` / RAM disk): evaluate running integration
  scratch directories on APFS ramdisk (`scripts/dev-ramdisk.sh`). Keep only if measured gain > 15%.
- [ ] **S7.c** Caching compiled test binaries: for positive `.bn` tests compiled via `bnc`,
  cache output by hash of `(source.bn + bnc_hash + bn_rt_hash)` to avoid redundant `clang` links
  when re-running unchanged cases.

---

## SECTION M — Point-to-Point Migration Protocol

Normative protocol governing the migration of tests from legacy locations to the new layout.

### M.1 Ledger and Ratchet Gate
- `tests/migration-ledger.csv`: tracks every test from pre-migration inventory.
- Columns: `old_id`, `file:line`, `domain`, `assertion_profile`, `new_target`, `status` (`open`, `migrated`, `retired`), `verified_by`.
- Ratchet gate: missing test without `migrated` or `retired` status fails the build.

### M.2 Assertion Equivalence and Hardening
- For every migrated test, extract assertion profile (stdout exact vs substring, exit code, diagnostic code).
- Rule: exact outputs remain exact; substring assertions are tightened to exact; backends are never dropped.

### M.3 Differential Coverage (`cargo-llvm-cov`)
- Measure region and branch coverage of the domain before and after migration over the same production sources.
- Require: 100% of regions and branches covered by the old test set remain covered by the new set.

### M.4 Historical Defect Verification
- For isolated historical defects (K68, K6 and future K# items), confirm the new test case
  fails when the fix is reverted.

### M.5 Mutation Verification (In-Process / Unit Scope)
- Run `cargo-mutants` on production modules exercised by the domain's in-process tests.
- Require: every mutant killed by the old test set is killed by the new test set.
- Subprocess tests calling external `clang`/`bnc` are excluded from mutation loops to prevent
  false positives from static runtime caching and runaway build times.

---

## SECTION V — Verification Techniques and Oracles

### V.1 Independent Oracles
- Every `.out`/`.err` expected result is derived directly from `language/0.6/` specification.
- Where spec is ambiguous, file a normative gap and mark test `oracle: bni-observed` (shrink-only).
- Hand-computed expectations for numeric overflow, floating-point string representation, and UTF-8 boundaries.

### V.2 Real Component Testing
- Ensure zero mock policy holds for: SQLite, file system, loopback networking/TLS, child processes, clang, node.
- Complete inventory in `tests/README.md` identifying gaps in error-path coverage on real backends.

### V.3 Fuzzing Input Pipelines
- Target `frontend`: source -> lex -> parse -> semantic -> lower -> IR validate.
- Target `BNWeb` HTTP parser and `BNJson` parser.
- Scheduled CI jobs with fixed execution budget; any crash or panic minimized and added to regression suite.

### V.4 Concurrency Verification
- Risk map of shared state: `bn_rt/src/dispatch_sync.rs`, `arc_abi.rs`, `BNWeb` sessions and connection drain.
- Model checking with `loom` on synchronization primitives.
- ThreadSanitizer (TSan) scheduled runs in CI under Linux x86_64.
- Miri validation of safe fallbacks in `bn_rt` C ABI exports.

---

## SPRINT 8 — Transition, Final Verification and Closure

- [ ] **S8.a** Comparative validation: run complete legacy battery alongside new pipeline on Linux, macOS, and Windows.
- [ ] **S8.b** Full coverage reconciliation: confirm no branch or test lost across the entire workspace via `cargo-llvm-cov`.
- [ ] **S8.c** Close all rows in `tests/migration-ledger.csv`; remove the ledger and temporary migration gates.
- [ ] **S8.d** Update `scripts/test-battery.sh` and CI workflows (`.github/workflows/ci.yml`) to use the new pipeline.
- [ ] **S8.e** Final documentation: update `man/bni.1`, `man/bnc.1`, `tests/README.md`, and record timing benchmarks.
- [ ] **S8.f** Move `todo/bucket-0.6.6-test-system.md` to `done/` upon release.

---

## SECTION M.8 — Federated Domain Catalogs & Test Directory Restructuring (BDFL 2026-10-20)

Migrado do bucket 0.6.5 (antigos `H.S2` e `H.S4`):

- [ ] **M.8.a** Adoção da convenção idiomática de subdiretórios de teste:
  - Isolar testes unitários in-tree em subdiretórios dedicados `src/tests/` (ex: `crates/<crate>/src/tests/<module>_tests.rs`, incluídos no módulo de produção via `#[cfg(test)] mod tests;`), despoluindo os arquivos `.rs` de produção e eliminando pressão sobre o gate de 1000 linhas (`tests/source_size.rs`).
- [ ] **M.8.b** Catálogos de mensagens e constantes federados por domínio:
  - Substituir o modelo de catálogo global único (`share/bn/catalog/`) por catálogos locais organizados por domínio e crate.
  - Geração de constantes tipadas (`pub const`) por domínio para uso nativo em padrões de `match` e tabelas estáticas.
- [ ] **M.8.c** Revisão formal da regra H-D5 para invariantes de `expect`/`panic`:
  - Padronizar que mensagens de `expect(...)` e `panic!(...)` são invariantes internas de desenvolvedor, não mensagens para usuários finais.
  - Isenção formal no scanner de literais de texto, concentrando a catalogação obrigatória em erros de compilação, diagnóstico, CLI, logs e contratos de tipos.
