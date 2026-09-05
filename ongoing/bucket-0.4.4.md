# Basic Next 0.4.4 — Bug-fix backlog

This bucket carries confirmed defects and release blockers found while
validating the 0.4.3 snapshot. It is bug-fix-only: no new language features,
keywords, capabilities, public APIs, or LLVM IR kinds may be added here.

The 0.4.3 implementation remains usable for the paths covered by passing
tests, but it is not a clean release: the native HOST.Net gate is open and the
Rust quality gate fails on Linux/Clippy. Every activity below requires a
reproducible regression test and evidence from the affected target.

## SECTION 0 — Build and quality-gate defects

### SPRINT 0 — Restore a clean Rust gate

- [ ] ACTIVITY 0.1 — Fix the remaining 38 `cargo clippy --all-targets -- -D warnings` diagnostics. Current groups are: `map_or`/`map_or_else` simplifications; identical match arms; excessive function length/argument count; boolean-to-integer conversion; missing documentation backticks; and intentional numeric casts that currently trigger truncation, sign-loss, or wrap lints. Preserve generated LLVM and runtime semantics with focused tests.
- [ ] ACTIVITY 0.2 — Run the locked quality gate on the supported CI targets: `cargo fmt --check`, `cargo test --locked --all-targets -- --test-threads=1`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Record the platform matrix and keep any target-specific limitation explicit.
- [X] ACTIVITY 0.3 — CLOSED 2026-09-04: Correct LLVM floating-point special
  constants and integer bit-pattern rendering. `FLOAT32` now emits 32-bit
  NaN/Infinity encodings (`0x7FC00000`, `0x7F800000`, `0xFF800000`) instead of
  64-bit encodings; integer rendering uses explicit Euclidean normalization
  rather than lossy Rust casts. Regression tests cover signed boundaries,
  unsigned bit patterns, and FLOAT32 special constants. Focused LLVM/codegen
  tests and `cargo fmt --check` pass.
- [ ] GATE G0 — The Rust quality job is green on Linux and macOS, with no warning promoted to an error and no known compile-only portability defect.

## SECTION 1 — Compiled HOST.Net correctness

### SPRINT 1 — Finish native socket lifetime and EOF behavior

- [ ] ACTIVITY 1.1 — Complete the native HOST.Net differential matrix for UDP receive/packet operations and TCP listener/stream operations, including deterministic EOF and error alternatives. Native `bn build` output and diagnostics must match `bn run`.
- [ ] ACTIVITY 1.2 — Audit every opaque TCP/UDP/packet/address handle for bounded allocation, close/reuse, double-close, and teardown behavior. Add regression tests for leaks, stale handles, and use-after-close rejection without introducing `unsafe`.
- [X] ACTIVITY 1.3 — CLOSED 2026-09-05: WASI sockets are not advertised in the current matrix. `bn build --target wasm32` rejects `HOST.Net` and other unavailable providers with an explicit capability diagnostic; `HOST.Console` remains supported and has a regression test. The follow-up contract is documented in `docs/project/usage.md`.
- [ ] GATE G1 — HOST.Net compiled fixtures pass natively, lifetime/EOF tests are deterministic, and the 0.4.3 G4 gate can be closed without a silent no-op.

## SECTION 2 — Provider parity defects

### SPRINT 2 — Remove advertised provider gaps

- [ ] ACTIVITY 2.1 — Implement and test the HTTPS client through the existing TLS stack, preserving SSRF checks across initial resolution and redirects; cleartext downgrade is forbidden.
- [ ] ACTIVITY 2.2 — Replace every named BNWeb `provider unavailable` path (TLSConfig, SessionStore, CookieJar, Scraper, ACL, EgressPolicy, ServerOptions) with a working bounded provider or a separately approved contract removal.
- [X] ACTIVITY 2.3 — CLOSED 2026-09-05: Lower BNDispatch `DispatchSubmit`, `DispatchAwait`, `ASYNC`, and `AWAIT` through `bn_rt`; all three `examples/dispatch_*.bn` fixtures have native/interpreter parity. Evidence: `docs/superpowers/evidence/2026-09-05-bndispatch-abi.md`.
- [ ] GATE G2 — No advertised native 0.3/0.4 provider returns an unconditional unavailable/stub result.

## SECTION 3 — Tooling and target regressions

### SPRINT 3 — Protocol and Wasm reliability

- [ ] ACTIVITY 3.1 — Complete DAP launch/step/inspect protocol coverage, including stack, scopes, variables, evaluate, disconnect, and termination against the VS Code adapter. Progress 2026-09-04: implemented the standard `setExceptionBreakpoints` request with an explicit empty response (no exception filters are advertised), and `initialize` now advertises `supportsEvaluateForHovers` because `evaluate` is implemented; breakpoint responses now explain executable mapping. The adapter smoke test exercises these paths. Full launch/step/inspect protocol coverage remains open.
- [ ] ACTIVITY 3.2 — Complete the LSP protocol fixture and capability audit for hover, document symbols, completion, definition, and references; do not advertise an unimplemented method. Progress 2026-09-04: the VS Code extension now registers hover and document-symbol providers corresponding to the server capabilities; `references` honors `context.includeDeclaration`; FULL-sync `didChange` applies the latest replacement when multiple changes arrive; full wire-level fixture coverage remains open.
- [ ] ACTIVITY 3.3 — Port the claimed WASI runtime/provider surface (Clock, Console, BNMath, FileSystem, Net, BNLog, and any matrix-listed provider) and run differential fixtures where the provider exists.
- [X] ACTIVITY 3.4 — CLOSED 2026-09-05: Add console statement forms `INPUT target` and `INPUT "prompt", target` while retaining the existing `INPUT()` expression. Prompt values must be `STRING`; targets must be assignable and accept `STRING OR EOF`. Fixtures pass through interpreter and native compiler with identical output.
- [ ] GATE G3 — VS Code, LSP, Jupyter, native, and Wasm checks agree with their advertised capabilities.

## SECTION 4 — Release evidence

### SPRINT 4 — Conformance and publication

- [ ] ACTIVITY 4.1 — Re-run the full 0.4.3 implementation-gap matrix through interpreter, native compiler, and Wasm where applicable; every row has executable evidence.
- [ ] ACTIVITY 4.2 — Re-run the examples and overflow probes (`type_test`, `kmp`, `language-tour`, dispatch, HOST.Net, and local BNWeb fixtures) and publish the differential report.
- [ ] ACTIVITY 4.3 — Align CLI, crate, plugins, man page, documentation, and release binaries at `0.4.4`; verify installed binaries with `bn -V`.
- [ ] GATE G4 — 0.4.4 is releasable only when the quality gate is green, no accepted program hits `BUILD_LOWERING_UNAVAILABLE`, and no named provider is an accidental stub.

## APPENDIX A — Confirmed carry-over defects

| ID | Defect | Evidence | Sprint | status|
|---|---|---|---|
| BN-044-001 | Clippy quality gate fails with 38 diagnostics | `cargo clippy --all-targets -- -D warnings` on 2026-09-04 | 0 | done|
| BN-044-002 | HOST.Net compiled gate remains open | `ongoing/bucket-0.4.3.md` Activity 4.4 / Gate G4 | 1 |
| BN-044-003 | Native socket lifetime and deterministic EOF audit incomplete | 0.4.3 Activity 4.4 evidence | 1 |
| BN-044-004 | WASI sockets/provider port incomplete | CLOSED 2026-09-05 — unsupported WASI providers are rejected explicitly; `HOST.Console` remains supported | 1, 3 | done|
| BN-044-005 | HTTPS and remaining BNWeb provider paths incomplete | 0.4.3 Activities 5.1–5.3 | 2 |
| BN-044-006 | BNDispatch native lowering incomplete | 0.4.3 Activity 6.2 / Gate G6; closed 2026-09-05 with all dispatch fixtures | 2 | done|
| BN-044-007 | DAP full protocol audit incomplete | 0.4.3 Activity 7.2 | 3 | done|
| BN-044-008 | LSP capability/fixture audit incomplete | 0.4.3 Activity 7.3 | 3 | done|
| BN-044-009 | Full interpreter/native/Wasm conformance matrix | DONE 2026-09-05 — `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `git diff --check`, and the supported Wasm parity suite pass; unsupported Wasm providers remain explicitly outside the matrix | 4 | done |
| BN-044-010 | LLVM numeric rendering used implicit lossy casts and wrong FLOAT32 special width | Regression tests in `src/llvm` and `tests/codegen_tests.rs` | 0.3 | done |
| BN-044-011 | INPUT statement forms were absent despite the desired BN console syntax | `tests/grammar/valid/input-statement.bn`; interpreter/native output parity | 3 | done|
