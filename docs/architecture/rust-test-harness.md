# Rust test harness

## Status and intent

This document defines the migration of Basic Next's active Python test harness
to Rust. It does not change the language, IR, runtime, compiler, or supported
target surface. The purpose is to make `cargo test` the single owner of active
test execution while preserving or strengthening black-box coverage.

Rust and Basic Next are the default implementation languages. Shell remains
appropriate for repository gates and system integration. Active test and CI
code must not depend on Python when the same contract can be exercised in
Rust.

## Scope

The migration covers:

- `tests/test_compiler_parity.py`;
- `tests/test_capabilities.py`;
- `tests/test_lsp_protocol.py`;
- `tests/test_wasm_parity.py`;
- `tests/console_stdout_tty.py`;
- `scripts/differential_runner.py`;
- `scripts/support_matrix_report.py`;
- active CI commands, capability-catalog identifiers, and current architecture
  documentation that refer to those files.

Historical release notes keep their original commands as historical evidence.
The staged `.deploy-bn060/apply_book_nav.py` and ignored
`scripts/generate_llvm_goldens.py` are existing work outside this migration and
must not be modified.

## Test ownership

| Rust target | Contract |
| --- | --- |
| `tests/compiler_parity.rs` | Native `bni` versus `bnc` parity, target-support diagnostics, stdin and exit-code behavior |
| `tests/compiler_capabilities.rs` | Capability catalog schema, fixture evidence, IR inventory, runtime ABI exports, and support-matrix coverage |
| `tests/wasm_parity.rs` | Interpreter versus wasm32/Node parity |
| `crates/bni/tests/lsp_protocol.rs` | Framed JSON-RPC behavior of the real `bni lsp` process |
| `tests/parity.rs` | Existing cross-backend gates plus the Unix stdout-PTY/stdin-pipe contract |
| `tests/support/` | Test-only process execution, timeout, failure artifacts, temporary paths, and cross-backend helpers |
| `crates/bn_support_matrix` | Typed support-matrix report library and private CLI replacing the Python report script |

The tests remain integration or broad black-box tests. They execute real
`bni`, `bnc`, native artifacts, Node, LLVM tools, and `nm`; those seams are not
mocked.

## Process execution contract

The shared runner must:

1. accept a program, arguments, optional stdin bytes, and a timeout;
2. drain stdout and stderr while the child runs so a full pipe cannot deadlock;
3. terminate and reap a timed-out child;
4. return exit status, stdout, and stderr without converting arbitrary bytes to
   UTF-8 prematurely;
5. write a JSON failure artifact under `BN_FAILURE_ARTIFACT_DIR`, or the system
   temporary directory when unset, for non-zero exit and timeout;
6. use unique paths based on process id plus an atomic counter so tests can run
   in parallel.

The runner uses `wait-timeout` 0.2.1. Temporary directories are managed by a
small test-only RAII type using `std`; introducing a general temporary-file
dependency is unnecessary.

## PTY contract

The console-size test remains Unix-only. It uses safe `rustix` 1.1.4 PTY and
termios APIs, sets a window
of 80 columns by 24 rows, connects only stdout to the slave PTY, and keeps stdin
as a pipe. No project code or test code may introduce `unsafe` for this test.

## Support-matrix report

`bn_support_matrix` is a private workspace crate with a library and binary. The
library reads the existing capability catalog, enumerates the `Instruction`
and `Type` source declarations using the same structural rules as the Python
tool, builds a typed Cartesian inventory for `interpret`, `llvm-native`, and
`wasm32`, and exposes both summary and JSON serialization. The binary preserves
the existing summary output and `--json` interface.

The capability integration test calls the library directly. This prevents a
nested Python process while keeping the report independently usable through
`cargo run -p bn_support_matrix -- --json`.

## Coverage preservation

The baseline contains 25 Python test methods: 16 compiler-parity, five
capability, two LSP, and two Wasm methods. The capability catalog currently
contains 52 program rows. Migration is complete only when every unique
assertion and fixture/input class has an identified Rust owner.

A test is not redundant merely because another test uses the same fixture.
Removal is allowed only when the existing Rust test checks the same observable
exit status, stdout/stderr, diagnostic code, IR fragment, input boundary, or
protocol message. Stronger existing assertions may replace weaker Python
assertions; the mapping must be recorded in the migration bucket.

## Execution-time improvements

The rewrite applies these optimizations without reducing isolation or
coverage:

- compile a fixture once per test and reuse its artifact across input cases;
- reuse shared process and parity helpers rather than spawning wrapper tools;
- preserve independent Rust test functions so the standard harness can run
  them concurrently;
- avoid global temporary names and mutable shared state;
- deserialize stable JSON schemas into explicit Rust structs instead of
  repeatedly traversing dynamic values;
- remove duplicate CI invocations after the same Rust target is already part
  of the workspace test battery;
- remove overlapping tests only after assertion-level equivalence is proven;
- keep separate LSP processes per test because protocol-state reuse would
  trade speed for order dependence and flakiness.

Baseline and final wall-clock measurements use the same build artifacts and
test-thread setting. Performance is reported, not used to weaken correctness
checks.

## Migration and gates

The work proceeds fixture-first:

1. capture a green baseline for the active Python tests;
2. add a shell language-policy gate that initially detects active Python test
   or workflow references;
3. implement and test the Rust support runner;
4. port each suite while its Python source remains available for comparison;
5. run the Rust owner and its Python predecessor against the same fixtures;
6. remove the predecessor and update current references only after parity is
   established;
7. make the language-policy gate green;
8. run formatting, the migrated integration targets, focused Clippy with
   warnings denied, the language-policy gate, CI syntax checks, and diff
   hygiene. The BDFL explicitly excluded the full product suite from this
   migration bucket.

The CI installs Python only if another independently documented non-test need
requires it. No workflow may invoke Python for the migrated test contracts.

## Acceptance criteria

- No `.py` file remains under `tests/`.
- Active workflows contain no Python test command.
- `scripts/differential_runner.py` and `scripts/support_matrix_report.py` are
  removed after their Rust replacements pass.
- All 25 method-level contracts and all 52 capability rows have Rust evidence.
- Failure artifacts still contain command, status or timeout, stdout, and
  stderr.
- Native, Wasm, LSP, ABI-symbol, support-matrix, EOF, Unicode, PTY, and target
  rejection cases retain their observable assertions.
- No `unsafe` is introduced.
- The final suite passes the repository gates required by `AGENTS.md`.
