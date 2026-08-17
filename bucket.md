# Basic Next Delivery Bucket

This bucket contains the work required to move Basic Next 0.1 from a language
draft to an executable interpreter. It complements the GitHub Project: this
file records the technical dependencies and acceptance criteria for each sprint.

## Sprint 0 — Executable Core Specification

**Goal:** remove every semantic ambiguity that blocks a lexer, parser, or
interpreter implementation.

- [ ] Complete EBNF for declarations, statements, expressions, calls, member
  access, indexing, `NEW`, `DELETE`, and `INPUT()`.
- [ ] Define name resolution order: local binding, parameter, then `SELF` field.
- [x] Define numeric conversions with postfix `AS`, narrowing behavior, and
  range failures.
- [x] Define `EOF`, `OR` in a type declaration, and `IS` for alternative-value
  tests.
- [ ] Define teacher-style runtime errors, one diagnostic format for `bn check`,
  `bn run`, and `bn build`, and `BN` exit codes.
- [ ] Define the default `Console` contract: `WriteLine`, `Error.WriteLine`,
  `ReadLine`, `PRINT`, and `INPUT()`.

**Done when:** every construct used by `examples/rpn-calculator.bn` has a
complete grammar and runtime meaning.

## Sprint 1 — BN Front End

**Goal:** parse valid `.bn` source into a testable abstract syntax tree.

- [ ] Create the `BN` command-line skeleton.
- [ ] Implement UTF-8 lexer, comments, numeric literals, strings, operators,
  keywords, and source locations.
- [ ] Implement parser and AST for the Sprint 0 grammar.
- [ ] Produce clear syntax diagnostics with file, line, column, and source span.
- [ ] Implement the zero-config path: `bn run hello.bn` without a manifest.
- [ ] Add lexer and parser conformance cases based on the language specification.

**Done when:** `BN check examples/rpn-calculator.bn` succeeds and malformed
source produces useful diagnostics.

## Sprint 2 — Semantic Analysis and Runtime

**Goal:** execute Basic Next programs safely through an AST interpreter.

- [ ] Resolve modules, imports, exports, classes, interfaces, fields, methods,
  and visibility.
- [ ] Implement primitive values, fixed vectors, arithmetic, comparisons,
  logical operations, and bitwise operations.
- [ ] Implement constructors, `SELF`, method calls, `NEW`, `DELETE`, typed
  pointers, bounds checks, and deleted-allocation checks.
- [ ] Implement `Console`, `PRINT`, `INPUT()`, `EOF`, runtime diagnostics, and
  process exit codes.
- [ ] Execute the RPN calculator and its normal/error scenarios.

**Done when:** `BN run examples/rpn-calculator.bn` works interactively and
handles invalid input, division by zero, stack overflow, and end of input.

## Sprint 3 — Conformance and First Release

**Goal:** make Basic Next 0.1 reproducible and usable by contributors.

- [ ] Turn every accepted language example into a conformance test.
- [ ] Add a concise `BN` command reference and getting-started guide.
- [ ] Define versioning, release notes, and the `v0.1.0` release checklist.
- [ ] Verify a clean clone can check and run every example.

**Done when:** Basic Next 0.1 has a tagged, documented, reproducible interpreter
release.

## Deferred Backlog

These items are valuable but do not block the interpreter milestone:

- `STRUCT`, variable-size collections, and generic classes.
- File, network, and other host capabilities.
- Package manifest, dependency resolution, and the package registry.
- DataFrame and missing-data facilities.
- GPU, DOM, Wasm, JIT, and native compilation targets.
