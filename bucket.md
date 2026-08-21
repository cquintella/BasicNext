# Basic Next Delivery Bucket

This bucket turns the implementation-readiness analysis in [analise.md](analise.md)
into executable work. It complements the GitHub Project: each numbered item is
a candidate card, while this file records dependencies and acceptance criteria.

## Pending design decisions

These decisions block specification completion or later runtime work. A
proposal is not an accepted language rule until the maintainer records that
acceptance in the normative specification.

### Basic Next 0.1

- [ ] **Numeric semantics.** Accept, revise, or reject
  [Checked Numeric Semantics](docs/proposals/numeric-semantics.md), then move
  the chosen policy into `docs/language/0.1.md` and add conformance fixtures.
  This covers promotion, checked overflow, exponentiation, shifts, conversion
  from `NAN` and infinity, and allocation-size overflow.
- [ ] **Pointers, ownership, and memory management.** Define ownership of a
  `NEW` allocation, aliasing, scope-exit destruction, `DELETE`, destructors,
  use-after-delete, and the memory-management strategy (including ARC or
  reference-counting cycle behavior).
- [ ] **Default runtime safety.** Define null-pointer behavior and the stable
  diagnostics for bounds, stale handles, double deletion, and allocation
  failures. Vector bounds are already runtime errors.
- [ ] **Program style guide.** Define canonical conventions for types,
  functions, methods, variables, constants, and identifier examples. The
  current grammar permits ASCII letters, digits after the first character, and
  `_`; it rejects `$`, `-`, `+`, and `!` in identifiers.
- [ ] **Paradigm boundary.** State the intended relationship among imperative,
  object-oriented, and functional programming. The current 0.1 draft is
  object-oriented and imperative, with function values but no lambdas or
  closures.

### Post-0.1 capability decisions

- [ ] **Standard packages.** Define packages and contracts for files, web
  server, and utilities.
- [ ] **`HOST.network`.** Propose a portable capability contract for sockets,
  address values, name resolution, connection lifetime, and diagnostics before
  adding network syntax or APIs.
- [ ] **Function pointers and FFI.** Re-evaluate pointer-to-function support
  only with a concrete interoperability requirement; ordinary BN callbacks use
  `FUNCTION(...) AS ...` values.
- [ ] **`HOST` concurrency and devices.** Define threads and GPU devices
  through a stable capability contract; network, file, GPU, DOM, and optional
  `HOST` capabilities remain outside the 0.1 interpreter milestone.

### Accepted compiler direction

BN will generate both native code and WebAssembly after 0.1. The compiler must
reuse the validated front end and preserve interpreter/compiler conformance.
Target-specific artifact formats, supported host capabilities, and portability
guarantees remain compiler-workstream decisions.

### Future exploration — do not schedule for 0.1

- [ ] WebAssembly libraries.
- [ ] Embedding BN in other languages.
- [ ] Parser-generator evaluation (ANTLR4, Flex/Bison, Peggy, Chevrotain,
  Parsimmon, Nearley, and Truffle/GraalVM). The current implementation plan
  remains a handwritten lexer and recursive-descent parser.
- [ ] Confirm and document the block-structured-language model when the scope
  and name-resolution rules are specified.

## Delivery rule

**The complete, testable EBNF is the first deliverable.** No Rust lexer, parser,
AST, or interpreter work starts before Sprint 0 is accepted. The reference
implementation must follow the specification; it must not decide missing
language behavior.

## Scope and acceptance baseline

The 0.1 scope and release gates are accepted in
[`docs/project/WBS-0.1.md`](docs/project/WBS-0.1.md).

- [x] Freeze the 0.1 in-scope deliverables and explicit exclusions.
- [x] Define objective acceptance gates for the EBNF, lexical analyzer,
  grammar analyzer, AST, semantic analyzer, interpreter, diagnostics, and
  conformance suite.
- [ ] Produce implementation evidence for those gates in the corresponding
  sprints.

### Stage deliverables and preliminary effort

Effort is estimated in person-days for one experienced contributor. Ranges are
planning aids, not commitments; implementation evidence changes the estimate.

| Stage | Deliverable | Definition of done | Effort |
| --- | --- | --- | ---: |
| Scope baseline | Approved 0.1 scope and gates | In/out scope and release criteria approved | 1–2d |
| EBNF | Normative grammar and fixtures | No undefined or conflicting production remains | 3–5d |
| Lexer | Rust lexical analyzer | Lexical fixtures produce expected tokens/errors | 3–5d |
| Grammar analyzer | Parser and syntax AST | Valid/invalid grammar fixtures behave correctly | 5–8d |
| Semantic analyzer | Symbols, types, flow, validated AST | Semantic fixtures resolve or diagnose correctly | 8–12d |
| Interpreter | Tree-walk Rust runtime | Approved 0.1 examples execute reproducibly | 8–15d |
| Runtime extensions | Objects, memory, modules, HOST | Lifecycle and capability checks pass | 10–20d |
| Release | Conformance suite and `v0.1.0` | Clean clone reproduces documented results | 5–8d |
| Compiler (post-0.1) | IR, backend, `bn build` | Compiled/interpreted parity is demonstrated | TBD |

The implementation pipeline is:

```text
.bn source → lexical analyzer → tokens and spans → grammar analyzer/parser
→ syntax AST → semantic analysis → validated AST, symbols, and types
→ interpreter

The compiler is a separate post-0.1 consumer of the same validated front end:

validated AST, symbols, and types → compiler IR → native-code or WebAssembly backend
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
- [x] Define every `END <KEYWORD>` pair, including `STRUCT` and the
  `FUNCTION` terminator used by constructors and destructors, and add
  mismatched-terminator examples.
- [x] Define the EBNF dialect, reserved-word tokenization, and the lexer
  precedence between reserved words, `NAN`, and identifiers.
- [x] Accept `SELF.member` and `SELF.member[index]` as assignment targets.
- [x] Decide whether reserved-but-unimplemented `EXTENDS` remains lexical-only
  or is removed from the 0.1 reserved set.

### 0.2 Lexical grammar

- [x] Define token classes for identifiers, keywords, integer literals,
  floating-point literals, `NAN`, strings, symbols, `NEWLINE`, and EOF.
- [x] Decide whether `NAN` is a dedicated literal token or a valid identifier;
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
- [x] Define callable `FUNCTION(...) AS ...` values and their 0.1 boundary.
- [x] Define multidimensional fixed-size vectors as `TYPE[length][...]`, with
  chained indexing and shape-checked nested literals.
- [x] Define whether vectors of object and interface references are allowed,
  and their initialization requirements.
- [x] Define pointer element types explicitly: 0.1 supports numeric elements
  only and rejects named types, `BOOLEAN`, and `STRING`.

### 0.4 Statements and expressions

- [x] Define complete productions for `LET`, `CONST`, assignment, call,
  `PRINT`, `INPUT`, `NEW`, `DELETE`, `RETURN`, `EXIT`, `CONTINUE`, and `STOP`.
- [x] Define `IF`, `ELSE IF`, `ELSE`, `WHILE`, `REPEAT`, counted `FOR` with
  optional `STEP`, and `FOR EACH` as complete block productions.
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
  type, exponentiation types, and conversion of `NAN` or infinity to integers.
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

- [x] Create `docs/language/diagnostics.md` with diagnostic structure, source
  spans, severity, stable identifiers, and teacher-style wording rules.
- [ ] Define errors for invalid syntax, names, types, missing returns, invalid
  `EXIT`, conversions, overflow, and invalid alternative-value use.
- [x] Define the `Math` contract in `docs/library/math.md`; keep language syntax
  in `0.1.md`.
- [ ] Move `Console` and `Error` contracts into separate standard-library
  documentation; keep language syntax in `0.1.md`.
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

## Sprint 3 — Grammar analyzer (parser) and syntax AST

**Goal:** implement the grammar analyzer that parses every valid 0.1 source
program into a source-spanned syntax AST and rejects invalid structure.

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
- [ ] Validate `EXIT` and `CONTINUE` against the enclosing loop stack.
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
  `REPEAT`, `FOR`, `FOR EACH`, `RETURN`, `EXIT`, and `CONTINUE`.
- [ ] Model internal control flow as normal continuation, return value, loop
  exit, and loop continuation; do not use Rust panics for BN program control flow.
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

## Post-0.1 — Compiler track

**Goal:** add compilation without creating a second language implementation.
The compiler must reuse the lexer, grammar analyzer, AST, semantic analyzer,
diagnostics, and conformance suite used by the interpreter.

- [ ] Define native-code and WebAssembly artifact contracts, supported host
  capabilities, and portability requirements.
- [ ] Define a minimal compiler IR from the validated AST; do not lower directly
  from tokens or source text.
- [ ] Reuse the same name resolution, type checking, overflow rules, and
  capability checks as the interpreter.
- [ ] Implement a compiler driver and an explicit `bn build` artifact contract.
- [ ] Add native-code generation and WebAssembly generation.
- [ ] Verify interpreter/compiler behavioral parity with the conformance suite.
- [ ] Document unsupported host capabilities and diagnostics at compile time.

**Done when:** a compiled BN program and an interpreted BN program produce the
same documented results for the shared conformance suite.

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
| Grammar analyzer/parser, Pratt expressions, AST and block diagnostics | Sprint 3 |
| Name resolution, type checking, narrowing, return analysis | Sprint 4 |
| Core execution and console | Sprint 5 |
| Classes, interfaces, pointers, deletion, static initialization, `HOST` | Sprint 6 |
| OO, heap, modules, host, conformance, release | Sprint 7 |
| Compiler IR, backend, `bn build`, and interpreter/compiler parity | Post-0.1 compiler track |

## Deferred backlog

These items remain outside the 0.1 interpreter milestone:

- `MATCH`, `ENUM`, inheritance, generic classes, and variable-size collections.
- Async/await, concurrency, and JIT.
- Native-code and WebAssembly compilation remain post-0.1; see the compiler
  track above.
- File, network, GPU, DOM, and optional host capabilities.
- C FFI capability, logical-library resolution, and a fixed-signature C ABI
  profile; see `docs/proposals/c-ffi.md`.
- Package manifest, dependency resolution, and package registry.
- DataFrame and broader missing-data facilities beyond the core `NA` value.
- Formatter, LSP, and editor integration; revisit after grammar stabilization.
