# Basic Next Delivery Bucket

This bucket turns the implementation-readiness analysis in [analise.md](analise.md)
into executable work. It complements the GitHub Project: each numbered item is
a candidate card, while this file records dependencies and acceptance criteria.

## Delivery rule

**The complete, testable EBNF is the first deliverable.** No Rust lexer, parser,
AST, or interpreter work starts before Sprint 0 is accepted. The reference
implementation must follow the specification; it must not decide missing
language behavior.

The implementation pipeline is:

```text
.bn source → lexer → tokens and spans → parser → AST → semantic analysis
→ validated AST, symbols, and types → interpreter
```

## Sprint 0 — Authoritative EBNF and lexical contract

**Goal:** publish one complete grammar that accepts every intended 0.1 program
and rejects every invalid construct used in conformance cases.

**Status:** in review. The documented grammar corrections and fixtures are
ready for a final consistency pass; they become executable checks when `bn
check` exists.

### 0.1 Grammar authority and scope

- [x] Complete `docs/language/0.1.ebnf` as the machine-readable, normative
  grammar source. Keep `docs/language/0.1.md` as the explanatory specification
  and link each rule to `0.1.ebnf`.
- [x] Remove the catch-all `statement = { non-newline-character } newline`
  production from the normative grammar.
- [x] Define `program`, module order, imports, declarations, blank lines, and
  whether the final source line receives an implicit `NEWLINE` before EOF.
- [x] Define the legal locations of blank lines and comments inside every block,
  including `CLASS`, `STRUCT`, `INTERFACE`, and function bodies.
- [x] Define every `END <KEYWORD>` pair, including `STRUCT`, `CONSTRUCTOR`,
  and `DESTRUCTOR`, and add mismatched-terminator examples.
- [x] Define the EBNF dialect, reserved-word tokenization, and the lexer
  precedence between reserved words, `NaN`, and identifiers.
- [x] Accept `SELF.member` and `SELF.member[index]` as assignment targets.
- [x] Decide whether reserved-but-unimplemented `EXTENDS` remains lexical-only
  or is removed from the 0.1 reserved set.

### 0.2 Lexical grammar

- [x] Define token classes for identifiers, keywords, integer literals,
  floating-point literals, `NaN`, strings, symbols, `NEWLINE`, and EOF.
- [x] Decide whether `NaN` is a dedicated literal token or a valid identifier;
  make its lexical status consistent in all documents.
- [x] Define decimal, binary, and hexadecimal literal syntax, invalid digits,
  leading zeros, overflow during lexing, and allowed floating-point forms.
- [x] Define the exact rejection rule for malformed numeric candidates such as
  `0b102`, `0xG`, `123name`, `.5`, and `1.`.
- [x] Define invalid UTF-8 behavior, string control characters, invalid escapes,
  unterminated strings, and unterminated block comments.
- [x] Define whether newlines inside a block comment act as whitespace only or
  produce statement-separating `NEWLINE` tokens.
- [x] Remove unused punctuation from the lexical surface or give it grammar
  meaning, including `{` and `}`.
- [x] Confirm maximal-munch rules for all overlapping symbols and comments.

### 0.3 Type grammar

- [x] Separate `return-type`, `variable-type`, `field-type`, and
  `parameter-type`; allow `VOID` only as a return type.
- [x] Make `NULL`, `NA`, and `EOF` valid alternative-type members in grammar.
- [x] Define named, vector, pointer, and alternative types without ambiguity.
- [x] Define multidimensional fixed-size vectors as `TYPE[length][...]`, with
  chained indexing and shape-checked nested literals.
- [x] Define whether vectors of object and interface references are allowed,
  and their initialization requirements.
- [x] Define pointer element types explicitly, including the intended support
  or exclusion of `BOOLEAN` and `STRING`.

### 0.4 Statements and expressions

- [x] Define complete productions for `LET`, `CONST`, assignment, call,
  `PRINT`, `INPUT`, `NEW`, `DELETE`, `RETURN`, `EXIT`, and `STOP`.
- [x] Define `IF`, `ELSE IF`, `ELSE`, `WHILE`, `REPEAT`, counted `FOR`, and
  `FOR EACH` as complete block productions.
- [x] Define assignment targets: identifier, member access, and indexing.
- [x] Define `STOP expression` as whole-program termination with a portable
  operating-system exit code.
- [x] Define expression grammar for literals, names, vectors, parentheses,
  calls, member access, indexing, `NEW`, postfix `AS` casts, unary operators,
  and binary operators.
- [x] Define the precedence of postfix `AS` casts relative to calls, member
  access, indexing, and arithmetic.
- [x] Define the syntactic distinction between equality `=` in expressions and
  assignment `=` at statement level.
- [x] Define expression-list syntax for `PRINT` and argument-list syntax for
  calls and constructors.
- [x] Permit every declared `data-type` in an `IS` type test; semantic analysis
  must still require it to be an alternative of the tested binding.

### 0.5 Grammar verification

- [x] Add valid and invalid grammar fixtures for `SELF` assignment, primitive
  `IS` tests, malformed numeric candidates, and `STOP`.
- [ ] Verify every official example parses under the grammar with an executable
  checker; this remains blocked until `bn check` exists.
- [x] Add explicit fixtures for blank lines, final newline, comments, cast
  precedence, `ELSE IF`, nested blocks, mismatched `END`, and invalid lvalues.
- [x] Review the grammar against `docs/language/keywords.md` so every reserved
  form is either grammatically accepted or explicitly documented as future-only.

**Done when:** `0.1.ebnf`, `0.1.md`, the keyword registry, and all examples
describe the same accepted source language. A parser can be implemented without
inventing any syntax rule.

## Sprint 1 — Static semantic contract

**Goal:** define the meaning of every syntactically valid core program before
implementing the frontend.

### 1.1 Names, scopes, and declarations

- [ ] Define scope nesting, shadowing, duplicate declarations, and the order of
  local binding, parameter, field, imported alias, and module-level lookup.
- [ ] Define visibility, private-member access, duplicate member names,
  overload policy, and whether a class name is valid as a value.
- [ ] Define import resolution, export visibility, module identity, import
  cycles, and the rule for exactly one executable `Start` function.

### 1.2 Types, values, and conversion

- [ ] Define compatibility, assignment, equality, and comparison rules for all
  primitive, vector, struct, class, interface, pointer, and alternative types.
- [ ] Define numeric promotion, widening, narrowing, overflow, division result
  type, exponentiation types, and conversion of `NaN` or infinity to integers.
- [ ] Define left-to-right evaluation order for operands and arguments.
- [ ] Define boolean short-circuit behavior for `AND` and `OR`, and evaluation
  rules for `XOR` and all bitwise operators.
- [ ] Define struct copy depth, class-reference assignment, vector assignment,
  and `CONST` behavior for mutable referenced values.

### 1.3 Alternative values and flow

- [ ] Define normalized alternative types: order, duplicates, and three-or-more
  alternatives.
- [ ] Define `IS` for `NULL`, `NA`, `EOF`, `Error`, and named object types.
- [ ] Define narrowing in `THEN` and `ELSE`, and when reassignment invalidates
  a narrowing fact.
- [x] Require a compatible `RETURN` on every completed path of a non-`VOID`
  `FUNCTION`.
- [ ] Specify control-flow analysis for `IF`, loops, `RETURN`, and `EXIT`.
- [ ] Define unreachable-code policy and diagnostics after `RETURN` or `EXIT`.

### 1.4 Diagnostics and standard surface

- [ ] Create `docs/language/diagnostics.md` with diagnostic structure, source
  spans, severity, stable identifiers, and teacher-style wording rules.
- [ ] Define errors for invalid syntax, names, types, missing returns, invalid
  `EXIT`, conversions, overflow, and invalid alternative-value use.
- [ ] Move `Console`, `Math`, and `Error` contracts into separate standard
  library documentation; keep language syntax in `0.1.md`.
- [ ] Define `PRINT`, `INPUT`, `EOF`, `Error`, `TryParse`, and `Parse` as
  normative standard-library/runtime contracts.
- [ ] Define shared diagnostic format and exit codes for `bn check`, `bn run`,
  and `bn build`.

**Done when:** every valid core AST has one deterministic type and execution
meaning, and every invalid core AST has a defined diagnostic.

## Sprint 2 — Rust lexer

**Goal:** turn source bytes into a fully spanned, conformant token stream.

- [ ] Create the Rust `bn` crate and source-location primitives.
- [ ] Implement a handwritten linear UTF-8 scanner; do not use a parser
  generator for lexing.
- [ ] Preserve `NEWLINE` tokens, update positions through block comments, and
  insert the specified final newline behavior.
- [ ] Implement keywords, identifiers, literal forms, strings, comments, and
  maximal-munch operators from Sprint 0.
- [ ] Add lexer fixtures for every accepted and rejected lexical form.
- [ ] Render lexical diagnostics with file, line, column, source excerpt, and
  a teacher-style explanation.

**Done when:** every Sprint 0 lexical fixture produces the expected token stream
or the expected diagnostic with a source span.

## Sprint 3 — Parser and syntax AST

**Goal:** parse every valid 0.1 source program into a source-spanned AST.

- [ ] Implement recursive descent for modules, declarations, blocks, statements,
  type references, and assignment targets.
- [ ] Implement a Pratt parser for the Sprint 0 expression grammar.
- [ ] Represent `ELSE IF` as branches of one `IF` node.
- [ ] Build AST nodes for modules, imports, declarations, statements,
  expressions, and type references; every node carries a `Span`.
- [ ] Keep source names in the syntax AST; do not create a second IR yet.
- [ ] Use `enum` and `match` for the first Rust AST implementation.
- [ ] Diagnose wrong terminators using a block stack and synchronize at the
  grammar points defined in Sprint 0.
- [ ] Add parser snapshots for all positive and negative grammar fixtures.

**Done when:** the parser accepts every conforming example, rejects every
negative grammar fixture, and preserves precise source spans in the AST.

## Sprint 4 — Semantic analyzer

**Goal:** resolve names, validate types and flow, and produce an executable AST
without interpreting invalid source.

- [ ] Build lexical scopes, symbol tables, `SymbolId`, and `TypeId`.
- [ ] Resolve imports, exports, functions, classes, structs, interfaces, fields,
  methods, and visibility.
- [ ] Normalize aliases (`INTEGER`, `FLOAT`) and alternative types.
- [ ] Validate assignment, conversion, operations, calls, vectors, pointers,
  constructors, and interface conformance.
- [ ] Implement alternative-type narrowing and invalidation rules.
- [ ] Implement structural return-path and unreachable-code analysis.
- [ ] Validate `EXIT` against the enclosing loop stack.
- [ ] Produce all Sprint 1 semantic diagnostics before execution.

**Done when:** every semantic fixture yields either a validated program with
resolved symbols/types or the specified diagnostic.

## Sprint 5 — Core interpreter

**Goal:** safely execute the minimum useful Basic Next subset through a Rust
tree-walk interpreter.

- [ ] Implement function frames, lexical environments keyed by `SymbolId`, and
  deterministic left-to-right evaluation.
- [ ] Implement primitive values, conversions, checked integer arithmetic,
  IEEE floats, Euclidean modulo, logical/bitwise operations, and shifts.
- [ ] Implement `LET`, `CONST`, assignments, expressions, `IF`, `WHILE`,
  `REPEAT`, `FOR`, `FOR EACH`, `RETURN`, and `EXIT`.
- [ ] Model internal control flow as normal continuation, return value, and
  loop exit; do not use Rust panics for BN program control flow.
- [ ] Implement vectors, `PRINT`, `INPUT`, `EOF`, `Error`, and the minimal
  Console/Math contracts.
- [ ] Implement zero-config `bn check file.bn` and `bn run file.bn`.

**Done when:** `bn run examples/hello.bn` and the core conformance suite run
with the documented results and diagnostics.

## Sprint 6 — Object, memory, module, and host semantics

**Goal:** close the semantics that carry the largest runtime and safety risk
before implementing the extended runtime.

### 6.1 Classes, structs, interfaces, and statics

- [ ] Define field initialization order, default values, constructor execution,
  constructor failure, and class reference semantics.
- [ ] Define method lookup, interface dispatch, compatible interface signatures,
  and behavior of static members through class names.
- [ ] Define static initialization order across modules and diagnostics for
  circular initialization.
- [ ] Define struct field initialization, copy semantics for nested values, and
  parameter-passing semantics.

### 6.2 Heap and pointers

- [ ] Define pointer default state, aliasing, base-versus-offset pointers, zero
  lengths, bounds checks, allocation failure, and runtime-sized regions.
- [ ] Define `NEW` and `DELETE` grammar/semantics for classes and typed memory.
- [ ] Define use-after-delete, double-delete, deletion during destruction, and
  behavior of aliases after deletion.
- [ ] Define destructor timing, process-end behavior, undeleted allocations,
  cycles, and partially constructed objects.
- [ ] State that the reference runtime uses checked allocation handles, never
  raw Rust pointers or `unsafe` memory access.

### 6.3 Host boundary

- [ ] Define the base `HOST` capability contract, `HOST.main`, and the `SYSTEM`
  interface or remove them from the 0.1 core.
- [ ] Define capability availability, denial, lifetime, and errors for
  host-supplied memory.
- [ ] Keep GPU, DOM, files, network, package registry, and other optional host
  capabilities outside the 0.1 interpreter milestone.

**Done when:** object allocation, invocation, static initialization, pointers,
deletion, modules, and host access can be implemented without runtime policy
decisions.

## Sprint 7 — Extended runtime and release

**Goal:** implement the remaining approved 0.1 semantics and publish a
reproducible reference interpreter.

- [ ] Implement structs, classes, constructors, methods, interfaces, and
  static initialization.
- [ ] Implement a generational checked heap for objects and pointers:
  slot, generation, declared type, length, liveness, and payload.
- [ ] Detect bounds violations, use-after-delete, double delete, stale handles,
  and invalid offset deletion.
- [ ] Implement destructors, module loading, exports, imports, and the defined
  base `HOST` contract.
- [ ] Turn every accepted example and every defined diagnostic into an
  implementation-independent conformance test.
- [ ] Add a concise BN command reference, installation guide, release notes,
  and `v0.1.0` release checklist.
- [ ] Verify a clean clone checks and runs all conformance programs.

**Done when:** Basic Next 0.1 has a tagged, documented, reproducible Rust
interpreter release and a conformance suite that another implementation can run.

## Post-0.1 — Parallel and heterogeneous computing

`PARALLEL` is reserved but deliberately excluded from the 0.1 grammar. The
candidate structured forms and open semantic questions are in
[`docs/proposals/parallel-computing.md`](docs/proposals/parallel-computing.md).
No implementation starts until its data-sharing, determinism, cancellation,
failure, and host/device-memory rules are specified.

## Traceability to `analise.md`

| Analysis finding | Bucket coverage |
| --- | --- |
| Incomplete EBNF, open statement, expressions, blocks, lexical ambiguity | Sprint 0 |
| Scope, types, alternatives, numeric rules, evaluation, diagnostics | Sprint 1 |
| Tokenization and source positions | Sprint 2 |
| Recursive-descent parser, Pratt expressions, AST and block diagnostics | Sprint 3 |
| Name resolution, type checking, narrowing, return analysis | Sprint 4 |
| Core execution and console | Sprint 5 |
| Classes, interfaces, pointers, deletion, static initialization, `HOST` | Sprint 6 |
| OO, heap, modules, host, conformance, release | Sprint 7 |

## Deferred backlog

These items remain outside the 0.1 interpreter milestone:

- `MATCH`, `ENUM`, inheritance, generic classes, and variable-size collections.
- Async/await, concurrency, JIT, Wasm, and native compilation targets.
- File, network, GPU, DOM, and optional host capabilities.
- C FFI capability, logical-library resolution, and a fixed-signature C ABI
  profile; see `docs/proposals/c-ffi.md`.
- Package manifest, dependency resolution, and package registry.
- DataFrame and broader missing-data facilities beyond the core `NA` value.
- Formatter, LSP, and editor integration; revisit after grammar stabilization.
