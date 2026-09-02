# Basic Next 0.4.2 Bug-Fix Bucket

This bucket defines the 0.4.2 maintenance release. The release is restricted
to bug fixes, diagnostic quality, regression coverage, and verification of
already accepted 0.4 behavior. It does not add language syntax, new runtime
capabilities, persistent Jupyter sessions, TLS reload, or new LLVM lowering
features.

The source of truth for the 0.4 contract remains [`docs/language/0.4/0.4.md`](../docs/language/0.4/0.4.md).
The 0.4.2 release must not silently expand the LLVM-supported subset. Programs
outside that subset must fail with a source-spanned, actionable diagnostic.

## Release objective

Make the 0.4.1 implementation trustworthy as a maintenance release by closing
the gap between unit-test success and user-visible behavior. In particular,
the test suite must distinguish language validity, interpreter execution, LLVM
lowering, native artifact generation, and execution of the generated artifact.

## SECTION 1 — Test effectiveness and capability inventory

### SPRINT 0 — Establish executable release evidence

- [X] ACTIVITY 0.1 — Create an explicit capability manifest for representative
  programs. Classify each fixture/example as `interpreter-supported`,
  `llvm-supported`, or `llvm-deferred`; record the expected exit status and
  output contract. `examples/kmp.bn` must remain `llvm-deferred` until the
  backend supports its user-defined calls and dynamic pointer arrays. The
  boolean-branch fixtures `print-if-boolean-expression.bn` and `print-if-or.bn`
  also remain `llvm-deferred` until the emitter stops producing duplicate SSA
  names in constant branches. `build-float-one.bn` remains deferred until
  interpreter and LLVM float formatting have the same observable contract.

  Objective: prevent `bn check` or `bn run` success from being mistaken for
  native compiler support.

  Dependencies: none.

  Acceptance criteria:

  - The manifest names every release smoke example and its supported commands.
  - A deferred example has an expected diagnostic, not an implicit expectation
    of successful native compilation.
  - Deferred LLVM fixtures are covered by focused negative tests that preserve
    the known limitation and do not silently enter the positive parity set.
  - The manifest is consumed by an automated test rather than being documentation-only.

  Verification: `tests/compiler-capabilities.json` and
  `tests/test_capabilities.py` validate paths, unique entries, support labels,
  and deferred diagnostics; the focused capability suite passes.

- [X] ACTIVITY 0.2 — Add end-to-end native build tests for every
  `llvm-supported` program. Each test must invoke `bn build <file> -o <artifact>`,
  verify successful toolchain completion, execute the artifact, and compare
  stdout, stderr, and exit code with the declared contract.

  Objective: detect failures that LLVM-text or frontend-only tests cannot see.

  Dependencies: Activity 0.1; configured native clang/LLVM toolchain.

  Acceptance criteria:

  - At least `hello.bn`, an integer-returning `Start`, a finite loop, printing,
    and seeded random are covered end to end.
  - A failed build cannot be reported as a passing artifact test.
  - Test artifacts are created only below the test temporary directory and are
    removed by the test cleanup path.
  - Tests do not use network access, wall-clock sleeps, or nondeterministic
    output.

  Verification: `tests/test_capabilities.py` builds and executes hello,
  integer-return, integer-print, and float-print artifacts; the focused suite
  passes with the configured native compiler.

- [X] ACTIVITY 0.3 — Add differential interpreter/native checks for deterministic
  programs. Run both `bn run` and the generated native artifact and compare
  observable output and process exit status.

  Objective: detect semantic drift between the interpreter and LLVM backend.

  Dependencies: Activity 0.2.

  Acceptance criteria:

  - Differences identify the source program and the first mismatching stream
    or exit status.
  - Input-dependent and nondeterministic examples are excluded or supplied
    with deterministic fixtures.
  - Integer `Start` return values are verified as native process exit codes.

  Verification: the capability suite executes `bn run` and the generated
  artifact for every `llvm-supported` entry and compares stdout and exit code;
  it passes for the current deterministic manifest.

### GATE G0 — Evidence boundary

The gate closes only when the capability manifest is executable, every claimed
LLVM example has a build-and-run test, and interpreter/native comparison is
available for deterministic programs. A passing unit-test count alone is not
gate evidence.

## SECTION 2 — LLVM diagnostics and regression contracts

### SPRINT 1 — Actionable failures without scope expansion

- [ ] ACTIVITY 1.1 — Define diagnostic assertions for unsupported LLVM IR.
  Every unsupported operation must report the diagnostic code, source path,
  line, column, enclosing function, operation, and the relevant callee/type
  when available.

  Objective: make a build failure explain what must change or which execution
  path is currently supported.

  Dependencies: none.

  Acceptance criteria:

  - A user-defined call names the callee, for example `KMPSearch`.
  - An unsupported allocation/index/vector reports the operation and type
    shape rather than only `allocation`, `indexing`, or `vectors`.
  - Provider calls identify the provider and capability.
  - The diagnostic recommends `bn run` only when interpreter execution is a
    valid alternative; it must not imply that native compilation succeeded.
  - Existing diagnostic codes and source locations remain stable unless a
    deliberate compatibility note is added.

  Verification: focused CLI tests for user-defined calls, arrays/pointers,
  provider calls, invalid `Start`, and missing `Start`.

- [ ] ACTIVITY 1.2 — Add a KMP regression test that validates the current
  contract boundary. The test must run `bn check` and `bn run` successfully,
  then assert that `bn build` fails with the exact actionable limitation for
  `KMPSearch` while the LLVM subset remains unchanged.

  Objective: ensure the original report remains visible and cannot regress to
  an opaque `calls` message or a false success.

  Dependencies: Activity 1.1.

  Acceptance criteria:

  - `bn check examples/kmp.bn` exits zero.
  - `bn run examples/kmp.bn` produces the expected match at index 10.
  - `bn build examples/kmp.bn` exits with the documented lowering diagnostic.
  - The diagnostic includes the source location and `KMPSearch`.

  Verification: focused KMP CLI test and manual command review.

- [ ] ACTIVITY 1.3 — Add regression coverage for the 0.4.1 integer entry-point
  fix. Test `Start() AS INTEGER` through LLVM emission, native artifact
  generation, and process execution for zero and non-zero exit codes.

  Objective: ensure the entry-point correction is tested at the user-visible
  boundary, not only by searching generated LLVM text.

  Dependencies: Activity 0.2.

  Acceptance criteria: `VOID` still exits zero; integer returns preserve the
  expected native exit code; parameters and unsupported return types retain
  clear diagnostics.

  Verification: focused CLI build-and-run tests plus the full Rust suite.

### GATE G1 — Diagnostic contract

The gate closes when unsupported programs fail deterministically with actionable
source-spanned diagnostics, supported programs build and execute, and the KMP
boundary is covered by a three-path (`check`/`run`/`build`) regression test.

## SECTION 3 — CI and release quality gates

### SPRINT 2 — Maintenance release verification

- [ ] ACTIVITY 2.1 — Add the native example and diagnostic suites to CI as
  mandatory checks. The job must use the repository's configured toolchain and
  must fail on a missing compiler, not silently skip native verification.

  Objective: make release verification reproducible on GitHub and local development.

  Dependencies: Gates G0 and G1.

  Acceptance criteria:

  - CI reports separately: Rust tests, native build/run tests, and diagnostic tests.
  - Toolchain absence is a visible failure with remediation instructions.
  - No generated binaries, temporary LLVM files, or local paths are committed.

  Verification: run the CI-equivalent commands locally and inspect the artifact list.

- [ ] ACTIVITY 2.2 — Perform the 0.4.2 release audit and update release notes
  with only verified fixes and known limitations. Reconcile README, 0.4 docs,
  capability manifest, and this bucket.

  Objective: prevent documentation from claiming native support that tests do
  not demonstrate.

  Dependencies: Activity 2.1.

  Acceptance criteria:

  - Documentation distinguishes interpreter support from LLVM support.
  - KMP is either promoted after actual end-to-end native evidence or remains
    explicitly deferred.
  - The release notes contain command-level verification evidence.

  Verification: documentation link check, `git diff --check`, and release review.

- [ ] GATE G2 — 0.4.2 release gate. All activities are complete; the following
  commands pass with no skipped required check:

  ```text
  cargo fmt --check
  cargo test --all-targets
  cargo clippy --all-targets -- -D warnings
  git diff --check
  ```

  The native build/run and diagnostic suites must also pass. The release is
  not complete if only the 159+ Rust unit/integration tests pass while an
  accepted user workflow remains untested.

## Definition of Ready

An activity is ready only when its expected command behavior, fixture inputs,
deterministic output, owner, and acceptance evidence are specified. Any request
to add LLVM capability is out of scope for 0.4.2 and requires a new versioned
decision rather than being smuggled into a bug-fix task.

## Definition of Done

An activity is done only after implementation and focused tests are present,
positive and negative paths are exercised, documentation and this bucket agree,
the required Rust checks pass, and the acceptance evidence is recorded in the
activity. No activity may be marked `[X]` from intent, code review, or a passing
unit test alone.
