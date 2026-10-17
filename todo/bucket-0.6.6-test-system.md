# Bucket 0.6.6 — Test system: speed, visibility, selection, verification

**Status:** Planned (todo). Not started. Was 0.6.7 until 2026-10-14, when
Carlos moved it ahead of BNFfi (now `todo/bucket-0.6.7.md`), so that BNFfi
is written in the new test layout and verified by sections V and M.
**Starts after:** `ongoing/bucket-0.6.5.md` closes (the M.1 ledger is
frozen on a suite that no longer changes under the 0.6.5 corrections).
**Tag:** `docs/releases/0.6.6.md` on close (test system; no language
change).

**Claim:** a faster test suite and inner loop (less build, I/O and process
contention), with every test visible, selectable and timed, keeping full
coverage and strict equivalence against the current harness; and a suite
whose expected results come from the specification, not from the
toolchain, that runs against the real components, fuzzes the inputs and
explores concurrency where the risk is (section V, Carlos, 2026-10-14).
Every current test migrates with its quality and coverage proved, domain
by domain, by the migration protocol (section M).

Property-based and mutation testing are not in this bucket: they are
`todo/proposals/property-and-mutation-testing.md`.

**Method:** sprints ordered so that what is simplest and usable at once comes
first, and each sprint gives the next one its measurements. Every new
technique runs alongside the old one; it replaces it only with a measured
gain and 100% of the old coverage.

## Invariants

- **Comparative execution:** no current test or harness is retired before
  its replacement runs alongside it and produces the same (or stronger)
  results. Section M is the method that proves it; "same results" without
  M's evidence is not a migration.
- **Semantics over speed:** no harness optimization changes error-assertion
  semantics or the W1–W5 contracts.
- **Split by function and domain** when a test module or helper is split
  (especially above 1 000 lines).
- **Measured, not estimated:** every claimed gain cites before/after numbers
  and the machine.
- **Independent oracle:** an expected result is derived from the
  specification (or computed by hand from it), never recorded by running
  `bni` or `bnc`. `bni` and `bnc` share the frontend, lowering and IR, so
  agreement between them proves nothing about a defect upstream of the
  backends.
- **No mocks:** a test of a component that talks to SQLite, the file
  system, the network, a child process, clang or node uses the real one.

## Facts (2026-10-08, updated 2026-10-14; macOS arm64 dev machine)

| Fact | Evidence |
| --- | --- |
| `scripts/test-battery.sh` already records per-binary wall time in `target/test-timings.txt` and orders the next run longest-first; `JOBS=1` because of the macOS scan stall (its header, measured 2026-09-22) | the script |
| Last battery: 64 binaries, 1 338 s; **42 binaries take 15–17 s each (582 s in total)**, including trivial ones (`lexer`, `bn_value`) | `target/test-timings.txt` |
| That floor is first-exec wait, not test work: `lexer` test binary, first run after linking `real 13.24 s`, `user 0.02 s`; second run `real 0.02 s` | `/usr/bin/time -p` |
| Built executables carry `com.apple.provenance`, not `com.apple.quarantine`: removing the quarantine attribute is a no-op | `xattr target/debug/bni` |
| `spctl developer-mode` is not available on this macOS | `spctl developer-mode` → "no action" |
| `tests/runtime.rs` already runs programs in process (`HostEnv`, 2 subprocess calls in 208 tests); `tests/semantic.rs` has none; `tests/parity.rs` has 103 (it needs the real `bni`/`bnc`) | `grep -c 'Command::new\|bni()\|bnc()'` |
| No test index. Measured 2026-10-14: 602 `#[test]` in `tests/*.rs`, 552 in `crates/*/src`, 296 `.bn` fixtures in `tests/grammar/` | `grep -c '#\[test\]'`, `find tests/grammar -name '*.bn'` |
| Some tests assert a substring, not the whole output: `.contains(` in `parity.rs` 29, `runtime.rs` 13, `semantic.rs` 6, `parity_regressions.rs` 6 | `grep -c '\.contains('` |
| `cargo-nextest`, `cargo-llvm-cov`, `cargo-mutants` not installed | each → "no such command" |
| 55 resolved K# (`done/known_issues-0.6.5-resolved.md`), 30 open (`known_issues.md`); this repository has 75 commits (re-initialized at 0.6.2), so a fix made before it, or mixed with other changes in one commit, cannot be reverted alone | `grep -c '^### K'`, `git log --oneline` |

---

## Test execution pipeline (design; Carlos, 2026-10-10)

The pipeline every test run follows, from a change to its evidence. Each
stage has an input, an output written to a file, a feedback line for the
person running it, and the adjustment its result triggers. Stages run in
order and stop at the first red one, except where a stage says otherwise.
The sprints below build its pieces (snapshot: 1b; per-feature runner: 1c;
per-test data: 3; index and selection: 5).

### Three depths

| Depth | When | Stages |
| --- | --- | --- |
| Inner loop | while editing | P0 (focused scope), P3, P4 for the touched crates, the one target being worked on |
| Item gate | before marking a bucket item `[X]` | P0–P7 on the touched crates and their interfaces (memory: bucket-item test scope) |
| Close gate | a sprint or bucket closes | P0–P9 in full, every platform, plus the V.3 fuzzing budget and the V.4 concurrency jobs |

### Stages

| Stage | Input | Output (under `target/pipeline/<run>/`, never tracked) | Feedback | Adjustment |
| --- | --- | --- | --- | --- |
| **P0 Scope** | the change (`git diff` against `HEAD`, or a bucket item) | `scope.json`: touched crates, spec files, test selection with the reason for each (`covers:` tag, crate, path) | one line: depth, number of tests selected, why | a shared core, the IR, `language/0.6/` or a test harness touched widens the scope to the full suite; Carlos can force full |
| **P1 Snapshot** | the working tree as Git sees it (`git ls-files -co --exclude-standard`) | `snapshot.txt`: source commit, hash of the copied file list | snapshot id printed at the start and end | a gate already running refuses the start (lock); edits during the run do not reach it (Sprint 1b) |
| **P2 Static gates** | the snapshot | one log per gate: `fmt`, `git diff --check`, `clippy -D warnings`, `source_size`, `shared_logic`, `check-shared-cores.sh`, forbidden deps, text literals (0.6.5 H) | `static: fmt=ok clippy=FAIL(3) …` and the first error of each red gate | red stops the run: fix the code, never raise a ceiling or grow an allow-list (ratchets only shrink) |
| **P3 Build** | the snapshot | build log; time; hash of `bni`, `bnc`, `libbn_rt.a` | `build: 41 s` or the first compile error | every later stage runs against these exact binaries (stale-binary guard: a test refuses binaries older than their sources) |
| **P4 Unit** | core and crate unit tests (`crates/*/src`) | JUnit with per-test duration | `unit: 1 210 passed, 0 failed, 38 s` + failing names | a failure is classified (P9) before anything else runs |
| **P5 Language conformance** | spec fixtures (`tests/grammar`, `semantic`, `validated_ir`, `ir`) | per fixture: verdict, diagnostic code, W1–W5 / `GC-*` tag | which spec section failed, with the diagnostic | a fixture that disagrees with the spec is a toolchain defect (fix the producer); a fixture that disagrees with a new spec text is updated in the same change as the spec |
| **P6 Cross-backend** | per-feature cases (`tests/features/`, Sprint 1c), `host_conformance`, BNWeb conformance, alternatives / ownership matrices | `matrix.json`: case × backend (`bni`, `bnc`, later `wasm32`) → pass / fail / declared unsupported, with the stdout, exit and diagnostic diff | a table of the red cells only; cells that now pass and are still listed as known failures | ratchet rules: a passing listed cell is removed from its `KNOWN_*` list; a new red cell is a defect, never a new list entry |
| **P7 Integration** | tests that use loopback network, `HOST.Exec`, servers, clang | JUnit; resources each test held (port, clang, temp dir) | red tests with their resource group | resource groups (nextest `test-groups`) serialize what shares a port or the CPU; a test that fails then passes on one retry is **reported as flaky**, never counted as green (0.6.5 Q.1) |
| **P8 Platforms** | the same snapshot on Linux, macOS, Windows (CI; Windows test host to reproduce) | per-platform summary + logs | `linux=ok macos=ok windows=FAIL(2)` | a platform-only failure is reproduced on that platform before any fix (AGENTS.md "Windows test host") |
| **P9 Report and classification** | outputs of P2–P8, duration history | `summary.txt` (one line per stage + failing names), `durations.csv` (appended, per test and machine), `flaky.txt` | the summary, pasted as evidence into the bucket item | every failure gets one class: **product defect** (`known_issues.md` K#, reproduced in the smallest program on both backends and on a clean `HEAD`), **test defect** (fix the test), **environment** (bucket Q item: tool missing, machine), **flaky** (cause studied, owner named; a flaky test stays visible until fixed); a duration regression past its history is reported |

### Rules

- Evidence names what it checked: the snapshot id and the binary hashes go
  with every result pasted into a bucket.
- No stage turns red into green by configuration: no skipped test, no
  raised ceiling, no added allow-list or known-failure entry.
- A failure is classified before it is fixed (P9); "the test is wrong" is a
  claim that needs the same reproduction as a product defect.

### Activities

- [ ] **PL.1** `tests/PIPELINE.md`: this design as the reference; AGENTS.md
  ("Changes and checks") points to it.
- [ ] **PL.2** `scripts/pipeline.sh` (and `.ps1` for the Windows host)
  driving P0–P9 at the three depths over the Sprint 1b snapshot; one
  summary line per stage.
- [ ] **PL.3** P0 selection from the Sprint 5 index (`covers:` tags), with
  the widening rules above; proved on three changes (a core, a backend
  file, a spec section).
- [ ] **PL.4** P6 `matrix.json` from the runners of Sprint 1c and the
  existing matrices; P7 flaky detection with one retry, reported.
- [ ] **PL.5** P9 classification template in `known_issues.md` and the
  bucket Q list; `durations.csv` history feeds the test-services program.
- [ ] **PL.6** Proved end to end: one red run per stage (a format error, a
  failing unit test, a parity cell, a flaky integration test) gives the
  feedback and adjustment the table names.

## SPRINT 0 — Prerequisites of section M (Carlos, 2026-10-14)

What must exist before any test moves: the tools the protocol uses, the
frozen inventory, the pilot, and the rule that keeps M.4 usable from now
on. The `covers:` tags are not here: until Sprint 5, M.5 selects modules
from the Sprint 1c feature `README.md`.

- [ ] **S0.a** (M.0) Install `cargo-nextest`, `cargo-llvm-cov`,
  `cargo-mutants`; record each version here. Verify each on the real
  workspace: the integration tests that spawn `bni`/`bnc`, and the native
  tests that link `libbn_rt.a` (AGENTS.md). A tool that cannot is reported
  here, and its layer (M.3 or M.5) is redesigned before Sprint 1c starts.
- [ ] **S0.b** (M.1) Freeze `tests/migration-ledger.csv` from
  `cargo test --workspace -- --list` and the fixture list, on the snapshot
  of Sprint 1b (or a clean tree if 1b is not ready), and add its gate.
- [ ] **S0.c** (M.6) Pilot domain: **K68**, comparison and `PRINT` of
  `STRUCT`, vector and object values — fix in commit `090de31` (9 files,
  one K#), tested on both backends, textual output with substring
  assertions to tighten. Fallback: **K6**, a call through an interface
  reference (`d9d0309`, 7 files). Measured 2026-10-14: these are the only
  K# fixes in this repository that are small commits of their own (K79 and
  K80 share `fd29d08` with script changes).
- [ ] **S0.d** Rule in AGENTS.md ("Changes and checks"): one K# per
  commit, with the production change separable from its test (separate
  commits, or the test in files the fix does not touch), so M.4 can revert
  the fix alone.

## SPRINT 1 — Baseline and inner loop (usable today, no code change)

- [ ] Record the current baseline here: one full `scripts/test-battery.sh`
  (total, per binary, machine), and `cargo test --no-run` build time.
- [ ] Inner-loop recipes in `scripts/test-battery.sh` documentation (or
  `tests/README.md`): per crate (`cargo test -p bn_frontend`, `-p bn_ir`,
  `-p bn_interp`), per target (`cargo test --test parity -- <name>`), and the
  reminder to `cargo build -p bni -p bnc -p bn_rt` before a focused target
  (AGENTS.md).

## SPRINT 1b — Gate on a snapshot (Carlos, 2026-10-09)

The gate (`cargo fmt --check`, `git diff --check`, `cargo build -p bni -p bnc
-p bn_rt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --all-targets`, tree unchanged after the tests) runs
in the working tree today, so any edit while it runs (about 30 min on the
macOS dev machine) mixes the evidence and forces a rerun; this happened
twice on 2026-10-09.

- [ ] `scripts/gate-snapshot.sh`: copy what Git sees, committed or not
  (`git ls-files -co --exclude-standard`), into a sibling directory
  `bn/basicnext-gate/` with `rsync` (timestamps kept, so builds stay
  incremental; its own `target/`, since a shared one would rebuild on every
  switch and contend for the lock); run the gate there; write the full logs
  to files and print one summary line (`fmt= diffcheck= build= clippy=
  test= tree= passed= failed=`) plus the names of failing tests.
- [ ] A copy, not a `git worktree`: a worktree sees only commits, and the
  gate must check the uncommitted state before the commit (AGENTS.md:
  commit only when asked). The snapshot records the source commit and a
  hash of the copied file list in the summary, so the evidence names what
  it checked.
- [ ] One gate at a time (the 2026-09-17 low-memory incident): the script
  refuses to start while another gate runs (a lock file); work in the main
  tree during a gate stays at `bni`/`bnc` runs and small focused tests.
- [ ] Proved: an edit in the main tree during a run leaves the gate result
  and `tree=` unchanged; a failing test is named in the summary; a second
  start is refused while one runs.
- [ ] `AGENTS.md` ("Changes and checks") names the script as the way to run
  the gate.

## SECTION M — Migration protocol (Carlos, 2026-10-14)

How every current test moves (Sprints 1c, 6, 7, 8) without losing what it
checks. Five layers, because each catches a loss the others miss: a test
dropped (M.1), a test that checks less (M.2), a code path no longer run
(M.3), a real defect no longer caught (M.4), a test that runs the code but
would not notice it is wrong (M.5).

**Unit of migration:** one domain at a time (a `tests/features/<feature>/`
directory of Sprint 1c, or a group of Sprint 6). The old and the new tests
of the domain run side by side; the old ones are removed only when
M.1–M.5 close for that domain, with the evidence (snapshot id, binary
hashes, the M.3 and M.5 numbers) pasted into the domain's item here.

**Order with V.1:** a test first moves with its old expectation unchanged;
only then is the expectation re-derived from the spec. A divergence is
classified (P9: product or test defect) and blocks that row until
resolved. The two steps never happen in one change: moving checks the
migration, re-deriving checks the oracle.

### M.1 Ledger (traceability)

`tests/migration-ledger.csv`, tracked while the migration runs, frozen
before the first move from `cargo test --workspace -- --list` and the list
of fixtures. Columns: old id (binary and test name, or fixture path),
`file:line`, kind (Sprint 5 kinds), assertion profile (M.2), new case(s),
status (`open`, `migrated`, `retired`), reason.

- A gate in `cargo test`, ratchet style (as `KNOWN_*_FAILURES` and
  `tests/source_size.rs`): an old id missing from the tree must have a row
  that is `migrated` (with its new cases present) or `retired` (with a
  reason: a duplicate proved by M.2 against a named case, or moved to a
  named test of another domain). Rows only close.
- The ledger is deleted at bucket close (Sprint 8), when every row is
  closed.

### M.2 Assertion equivalence

Per row, what the old test checks: stdout (exact or substring), exit code,
diagnostic code, line, stderr empty, backends (`bni`, `bnc`, `wasm32`).
The new case checks a superset: a substring may become an exact `.out`,
an exact output never becomes a substring, a backend is never dropped.
The profile is extracted semi-automatically from the Rust source and
reviewed per row; the Facts list the substring assertions to tighten.

### M.3 Differential coverage

`cargo-llvm-cov`, region and branch coverage, the old set against the new
set of the domain, over the same production sources, per crate: what the
old set covers, the new covers. Necessary, not sufficient: coverage shows
what runs, not what is checked (M.5 does).

### M.4 Historical defect replay

For each resolved K# whose fix is a commit of its own in this
repository: revert only its production files (a fix commit that also adds
the K#'s test keeps the test) in a separate `git worktree`, build, and
run the
K#'s case (V.1 regression rule) on both suites; both must fail. A K# that
cannot be reverted alone (fixed before 0.6.2, or mixed in one commit) is
listed as not covered by M.4, never assumed covered.

### M.5 Mutation comparison (migration verifier)

`cargo-mutants` on the production modules the domain's tests exercise
(selected through the Sprint 5 `covers:` tags; before Sprint 5 exists,
from the implementation sites the Sprint 1c feature `README.md` names),
once with the old tests, once with the new: every mutant the old tests kill, the new kill. Run on
Linux, per domain, not as a CI gate. Mutants that survive both suites are
recorded as test-gap items for
`todo/proposals/property-and-mutation-testing.md`; they do not block the
migration, which proves only that nothing was lost.

### Activities

- [ ] **M.0** Tools installed, versioned and verified: Sprint 0, S0.a.
- [ ] **M.1** Ledger frozen and its gate in `cargo test`: Sprint 0, S0.b.
- [ ] **M.2** Assertion profiles for every row; the extraction and the
  review rule written in `tests/README.md`.
- [ ] **M.3–M.5** Per domain, as each domain migrates (one item per domain,
  added when the domain starts).
- [ ] **M.6** Pilot: the K68 domain (fallback K6; Sprint 0, S0.c) through
  M.1–M.5 end to end, before any other domain starts; the protocol is
  corrected from what the pilot shows.

## SPRINT 1c — One directory per feature (Carlos, 2026-10-09)

Follows section M.


Today a behaviour is spread over `tests/grammar/valid/`, a hand-written test
in `tests/parity.rs` (2 213 lines) holding the expected output, and a row
in `tests/grammar/README.md`; `examples/` is not run at all
(`examples/patterns/singleton.bn` did not parse from 0.6.2 to 0.6.5, K29).

- [ ] Layout `tests/features/<feature>/` (e.g. `interfaces`, `alternatives`,
  `vectors`, `arc`, `host-env`): each case is `<case>.bn` plus its expected
  result as data — `<case>.out` (stdout), and for a refusal `<case>.err`
  (diagnostic code and line) and `<case>.exit`; a `README.md` names the
  spec section (`language/0.6/0.6.md#…`) the feature follows and its
  implementation sites (semantic module, IR instruction, interpreter file,
  LLVM file), so a feature can be worked on without the whole language.
- [ ] One data-driven runner (a Rust test) that runs every case on `bni`
  and `bnc` and compares with the files: a new case is a new file, no Rust.
  Native support refusals are not expected results (0.6.5: none remain).
- [ ] `examples/` (patterns included) runs through the same runner, with
  `.out` files beside the examples.
- [ ] Migration alongside (Invariants): cases move from `tests/grammar/` and
  `tests/parity.rs` feature by feature; a hand-written test is removed only
  when its case runs in the runner with the same expectation; the grammar
  fixtures used by the parser and lexer tests stay where they are.
- [ ] A gate: every case has its expected-result file; every directory has
  its `README.md` with a spec link that resolves.

## SPRINT 2 — macOS first-exec scan (largest measured cost)

Goal: remove the ~15 s wait each freshly linked test binary pays on macOS
(582 s of 1 338 s), which also forces `JOBS=1`.

- [ ] Identify the mechanism: confirm with `log stream` / `syspolicyd` /
  XProtect logs during a battery run which service holds the exec. TODO:
  not verified; the measurement shows the wait, not its cause.
- [ ] Try the documented Developer Tools exemption (System Settings →
  Privacy & Security → Developer Tools, adding the terminal / IDE that runs
  the tests). TODO: confirm the setting exists on this macOS and what it
  exempts. A machine setting, recorded in the developer setup docs, not in
  repository code.
- [ ] Re-run the battery: the floor of the 42 binaries and the total,
  before/after; then `JOBS=2` and `JOBS=4`; no `HOST.Exec` test fails with
  `EXEC_TIMEOUT`.
- [ ] Dropped with evidence: `xattr -d -r com.apple.quarantine target/`
  (the attribute is not present; see Facts).

## SPRINT 3 — `cargo-nextest` alongside the battery

Goal: per-test timing, one process per test, filtersets and concurrency
groups — the measurement and selection base for every later sprint.

- [ ] Install `cargo-nextest`; `.config/nextest.toml` with a JUnit output
  (per-test duration).
- [ ] A test group with low `max-threads` for the suites that invoke clang /
  LLVM or spawn `bni`/`bnc`.
- [ ] Full run with `cargo nextest run` next to `scripts/test-battery.sh`:
  same pass/fail set, total time and peak memory compared; failure reports,
  logs and timeouts at least as precise.
- [ ] Windows and Linux CI unchanged until Sprint 8.

## SPRINT 4 — Test profile `opt-level = 1`

- [ ] Measure build and run time with the current profile (from Sprints 1
  and 3).
- [ ] `[profile.test] opt-level = 1` in the root `Cargo.toml`; measure again.
- [ ] Keep it only if build + run is faster in total; identical results.

## SPRINT 5 — Test index and selection (data contract)

Goal: make every test visible and selectable, and give the test-services
program (below) a stable input. Needs Sprint 3.

- [ ] Doc-comment convention for every `#[test]`: a one-line description,
  then `covers:` tags naming what the test verifies (spec file and section,
  `GC-*` gate, W1–W5), e.g. `/// covers: bnweb.md#Errors, GC-SUP`. Written
  in `tests/README.md`.
- [ ] Ratchet gate in `cargo test`: a test without a description or
  `covers:` fails, except those in a shrink-only list (as
  `tests/source_size.rs` and `KNOWN_*_FAILURES` do). The list starts with
  every current test and only shrinks.
- [ ] Test kind, derived and not hand-written: `unit`, `bni` subprocess,
  `bnc` + clang, network (loopback), Windows-only.
- [ ] Index schema document (fields: id, binary, file:line, description,
  covers, kind, last durations by machine). Generated under `target/` from
  `cargo nextest list --message-format json`, the doc comments and JUnit;
  never tracked in Git (generated output; CI requires an unchanged tree).
- [ ] Selection recipes by `covers:` tag, name or kind as nextest filtersets
  (`-E`). The full battery stays mandatory at bucket close.

### Test-services program (parallel project, consumer of the index)

A separate program, developed in parallel in a sibling repository
(`bn/basic-next-apps` candidate), preferably in Basic Next (BNJson for
nextest output, BNSqlite for duration history, `HOST.Exec` to call nextest,
BNWeb for the visibility page once bucket 0.6.5e closes). It is
**advisory, never a gate**: CI runs `cargo test` / nextest directly, so a
defect in the program can never hide a failing test. It can start once
Sprint 5 fixes the index schema.

v0 scope, in order:

1. Inventory from the index.
2. Duration history per test and machine (median, trend, regressions).
3. Duplicate candidates by hash of the normalized embedded BN source plus
   expected output (exact, cheap). Coverage-based candidates
   (`cargo-llvm-cov` per test) later: equal coverage is a candidate, not a
   duplicate, since assertions may differ. Nothing is deleted automatically.
4. Selection: a change (`git diff`, a module name) mapped through `covers:`
   tags to a nextest filterset; execution stays with nextest.
5. Visibility page.

Out of v0: its own runner or scheduler (nextest does it); proposing
`test-groups` from measured durations and resources (port, clang,
`TMPDIR`), once enough history exists.

## SPRINT 6 — Consolidate integration binaries

Follows section M.

Goal: fewer test executables to link (and, on macOS, to scan on first
exec), decided by the Sprint 2 and 3 numbers.

- [ ] From the measurements, list the binaries whose cost is mostly fixed
  (link + first exec) rather than test work.
- [ ] Build `tests/suite/` alongside the current targets, grouped by
  function and domain (e.g. `suite/parity/`, `suite/codegen/`,
  `suite/semantic/`).
- [ ] Compare `cargo test --no-run` and full-run time before/after.
- [ ] Every test keeps its name and its individual filter.

## SPRINT 7 — Targeted harness work (driven by per-test data)

Follows section M.

The original "in-process harness" premise is outdated: `runtime.rs` already
runs in process and `semantic.rs` spawns nothing (Facts). Work goes where
the Sprint 3 data shows time.

- [ ] Rank the slowest tests by kind; for `bni`-subprocess tests that do not
  need the CLI surface, a dual-mode pilot (subprocess vs in process through
  the existing `HostEnv` path) with strict equivalence of diagnostics and
  output before expanding. Tests that exist to check the real `bni`/`bnc`
  binaries stay subprocess.
- [ ] `TMPDIR` on a RAM disk (`scripts/dev-ramdisk.sh`: create, mount,
  tear down), measured against the internal disk; kept only with a gain.

## SECTION V — Verification techniques (Carlos, 2026-10-14)

The sprints above make the suite fast and visible; this section makes it
able to find defects the current suite cannot. Each item states what it
catches that the existing tests do not.

### V.1 Behaviour and regression tests with an independent oracle

Catches: a defect in semantics or lowering that `bni` and `bnc` reproduce
identically, which a parity check reports as green.

- [ ] Rule in `tests/README.md` and in the Sprint 1c layout: every expected
  result file (`.out`, `.err`, `.exit`) names, in the case's header comment
  or the feature `README.md`, the spec section that determines it
  (`language/0.6/0.6.md#…` or a companion contract). Expected results are
  written from the spec, never produced by running the toolchain.
- [ ] Where the spec is silent, the order of authority applies (AGENTS.md:
  observable `bni` behaviour), but the case is marked `oracle: bni-observed`
  and the silence is reported as a `TODO (normative gap)` in the spec.
  These cases are counted in the summary and only shrink.
- [ ] Migration check (Sprint 1c): each expectation moved from
  `tests/parity.rs`, `tests/parity_regressions.rs` or `tests/grammar/` is
  re-derived from the spec when it moves; a mismatch is classified (P9)
  before the move, never copied.
- [ ] Regression rule: every closed `known_issues.md` K# leaves a case (the
  smallest program that reproduced it) with `covers: known_issues.md#K<NN>`
  and the spec section; closing a K# without that case is not closing it.
- [ ] Numeric and textual edge cases (overflow → `Error`, conversions,
  division, `PRINT` of `DOUBLE`, UTF-8 string operations, temporal) have
  hand-computed expectations with the computation written next to the
  case.
- [ ] Gate: a case without a spec reference or an `oracle:` mark fails
  (ratchet list for current cases, shrink-only).

### V.2 Integration with the real components

Catches: a defect in the boundary with the system (paths, errors,
encodings, timing) that a substitute would hide.

- [ ] Inventory in `tests/README.md`: per domain, the real component its
  tests use and the platforms where it runs — SQLite (`bn_core_sqlite`,
  `bn_lib_sqlite`, `bn_rt/src/sqlite_abi.rs`), file system (`bn_host_fs`,
  `bn_rt/src/secure_fs.rs`, `FileIdentity` TOCTOU), loopback network and
  TLS (`bn_core_net`, `bn_core_web`, `bn_rt/src/net_abi/`), child processes
  (`bn_host_exec`), clang/`wasm-ld`, node for Wasm. TODO: the inventory
  states which of these have no real-component test today.
- [ ] Each gap from the inventory gets a test against the real component,
  on `bni` and `bnc`, including the error paths the contract names
  (`bnsqlite.md`, `host.md`, `host-net.md`, `host-exec.md`).
- [ ] Rule: no mock, fake or stub replaces a component above; a test that
  cannot run where the component is missing fails with an environment
  classification (P9), never skips silently.

### V.3 Fuzzing the inputs

Catches: panics, hangs and unbounded memory on inputs no one wrote by
hand, and IR the validator should have rejected. Today `fuzz/` has two
targets (`lex`, `parse`), runs outside `cargo test` and is not in CI.

- [ ] Target `frontend`: source → semantic analysis → lower → `validate`
  (through the same entry `bni check` uses). Oracles: no panic, no hang
  past a time limit, and a program that semantic analysis accepts is
  accepted by `validate` (W1; a rejection there is a lowering defect).
- [ ] Targets for the external input parsers: HTTP request parsing
  (`bn_core_web/src/http.rs`), BNJson parsing, and the other `BN*` parsers
  that read untrusted bytes. TODO: list them from the modules before
  writing the targets.
- [ ] Seed corpus built at run time from `tests/grammar/valid/`,
  `tests/features/` and `examples/` (the `fuzz/corpus/` directory stays
  untracked, AGENTS.md).
- [ ] Every finding is minimized and becomes a permanent case run by
  `cargo test` (V.1 rules: spec reference, `covers:`), then is fixed at the
  producer; a finding is never silenced in the target.
- [ ] Budget and place: a scheduled CI job (nightly toolchain, as
  `cargo-fuzz` requires) with a fixed time per target, and the same budget
  at the close gate; neither is part of the ordinary `cargo test`. Record
  the budget and the executions per second per target.
- [ ] Out of this bucket, explicitly: a generator of well-typed programs
  for differential `bni` × `bnc` fuzzing (Csmith-like). TODO: decide
  whether it becomes its own proposal.

### V.4 Concurrency, by risk

Catches: data races, deadlocks and lost wake-ups in code that runs
concurrently: `ASYNC`/`AWAIT` with `Dispatch.Queue.Concurrent`
(`0.6.md#bounded-asyncawait`), the BNWeb server, and the runtime state
that compiled programs share.

- [ ] Risk map in `tests/README.md`: each module with shared state, its
  primitive and its risk (high: used by concurrent BN code or a server
  loop; low: process-wide state set once). Starting points found by
  search (2026-10-14): `bn_rt/src/dispatch_sync.rs` (`Mutex`/`Condvar`
  semaphore, barrier, mutex), `bn_rt/src/dispatch_abi.rs`,
  `bn_rt/src/arc_abi.rs` (process-wide `Mutex<ArcCore>`),
  `bn_core_dispatch`, `bn_core_web/src/web/{server,sessions,drain}.rs`,
  `bn_rt/src/net_abi/handles.rs`, `bn_rt/src/log_abi/`, `bn_interp`
  executor. TODO: confirm each entry by reading the code.
- [ ] High-risk primitives: model checking with `loom` on the
  synchronization core (first `dispatch_sync.rs`). TODO (decision): `loom`
  needs the module to use `loom::sync` under `cfg(loom)`, which touches
  production code; Carlos decides before the change.
- [ ] Stress tests on the real runtime for the high-risk paths: concurrent
  `ASYNC` submits, `AWAIT` with timeout, cancel, BNWeb under concurrent
  connections and shutdown while draining; on `bni` and `bnc`, with
  results checked against the spec (V.1), not only "no crash".
- [ ] ThreadSanitizer job on the stress tests and the concurrency unit
  tests (nightly, `-Zsanitizer=thread`). TODO: confirm the supported
  targets (Linux x86_64 in CI at least) and whether `libbn_rt.a` linked by
  `bnc` programs can be instrumented.
- [ ] Miri on the `unsafe` modules where it can run (pure Rust paths of
  the C ABI exports, `cpu.rs` fallbacks). Miri does not execute foreign
  calls or most system calls: `ffi.rs` (once bucket 0.6.7, BNFfi, adds it; it does
  not exist on 2026-10-14) and the sockets stay with the stress
  and TSan jobs; the uncovered `unsafe` sites are listed, not assumed safe.
- [ ] A race found is a product defect (K#) with the smallest program that
  shows it, and its fix leaves the stress case in `cargo test`.

## SPRINT 8 — Transition and coverage validation

- [ ] Full cross run: current suite vs the new pipeline, same results.
- [ ] Coverage report (`cargo-llvm-cov`): no branch or test case lost —
  the M.3 per-domain results aggregated over the workspace, the M.5 kill
  sets aggregated, the M.4 list (replayed and not replayable).
- [ ] Every ledger row closed (M.1); the ledger and its gate removed.
- [ ] `scripts/test-battery.sh` and project docs updated to the official
  pipeline; CI (Linux, macOS, Windows) moved where the gain is proven.
- [ ] Section V closed with evidence: the count of `oracle: bni-observed`
  cases, the real-component inventory with no open gap, the fuzzing budget
  run and its findings, the concurrency jobs green.
- [ ] Close with recorded time evidence; `git mv` this bucket to `done/`.
