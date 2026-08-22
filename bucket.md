# Basic Next Delivery Bucket

This bucket turns the implementation-readiness analysis in [analise.md](analise.md)
into executable work. It complements the GitHub Project: each numbered item is
a candidate card, while this file records dependencies and acceptance criteria.

## Active implementation objective

Build the Rust reference implementation for accepted Basic Next 0.1: `bn check`
and `bn run`, backed by a source-spanned lexer, handwritten parser, syntax AST,
semantic analyzer, typed BN IR, deterministic IR interpreter, and shared
conformance fixtures. Native/WebAssembly compilation and proposed libraries do
not belong to this objective. Accepted temporal types and the base `HOST`
contract do belong to 0.1.

## Pending design decisions

These decisions block specification completion or later runtime work. A
proposal is not an accepted language rule until the maintainer records that
acceptance in the normative specification.

### Basic Next 0.1

- [x] **Numeric semantics.** The accepted rules are recorded in
  `docs/language/0.1.md`; implementation conformance work remains in the
  runtime sprints.
- [x] **Pointers, ownership, and memory management.** The 0.1 specification
  defines ownership of a
  `NEW` allocation, aliasing, scope-exit destruction, `DELETE`, destructors,
  use-after-delete, and the manual checked-handle memory strategy.
- [x] **Default runtime safety.** The 0.1 specification defines null-pointer
  behavior and the stable
  diagnostics for bounds, stale handles, double deletion, and allocation
  failures. Vector bounds are already runtime errors.
- [x] **Base host contract.** `HOST.main` exposes immutable command-line
  arguments with entry `0` as the executable; `HOST.clock` exposes Unix-epoch
  milliseconds and monotonic nanoseconds. `HOST.memory` is deferred.
- [x] **Temporal value model.** `TIMESTAMP`, `DATE`, `TIME`, and `TIMEZONE`
  have value semantics; RFC 3339 is mandatory timestamp interchange and IANA
  TZDB identifiers name time zones.
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

BN will generate both native code and WebAssembly after 0.1 through LLVM. The
compiler must reuse the validated front end and preserve interpreter/compiler
conformance. The validated typed AST lowers first to a compact typed BN IR.
The reference interpreter executes that IR, and the later LLVM backend lowers
the same IR to LLVM IR. LLVM supplies target code generation, optimization,
object emission, linking integration, and any later JIT; BN retains its
frontend, semantic rules, diagnostics, IR, and runtime.

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
| IR and interpreter | Typed BN IR and Rust executor | Approved 0.1 examples execute reproducibly | 8–15d |
| Semantic closure | Module graph and complete accepted-type resolution | No executable expression remains unresolved | 5–8d |
| Extended IR | Modules, objects, pointers, temporal values, and capabilities | Full accepted AST lowers to validated BN IR | 4–7d |
| Module runtime | Imports, exports, statics, and initialization | Multi-module fixtures execute deterministically | 4–7d |
| Object runtime | Structs, classes, methods, interfaces, constructors | OO fixtures execute with documented value/reference behavior | 7–12d |
| Memory runtime | Checked pointers, deletion, destructors, heap integration | Lifecycle and checked-memory diagnostics pass | 5–9d |
| Host and temporal runtime | Arguments, clocks, RFC 3339, UTC, and TZDB | Capability and temporal conformance vectors pass | 5–9d |
| Release | Conformance suite and `v0.1.0` readiness | Clean clone reproduces documented results | 4–7d |
| Compiler (post-0.1) | IR, backend, `bn build` | Compiled/interpreted parity is demonstrated | TBD |

The implementation pipeline is:

```text
.bn source → lexical analyzer → tokens and spans → grammar analyzer/parser
→ syntax AST → semantic analysis → validated AST, symbols, and types
→ typed BN IR → interpreter

The compiler is a separate post-0.1 consumer of the same validated front end:

typed BN IR → LLVM IR → native-code or WebAssembly backend
```

## Sprint 0 — Authoritative EBNF and lexical contract

**Goal:** publish one complete grammar that accepts every intended 0.1 program
and rejects every invalid construct used in conformance cases.

**Status:** completed. The documented grammar is covered by executable lexer,
parser, and `bn check` fixture runs.

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
- [x] Verify every official example parses under the grammar with executable
  `bn check`.
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

- [x] Define scope nesting, shadowing, duplicate declarations, and the order of
  local binding, parameter, field, imported alias, and module-level lookup.
- [x] Define visibility, private-member access, duplicate member names,
  overload policy, and whether a class name is valid as a value.
- [x] Define import resolution, export visibility, module identity, import
  cycles, and the rule for exactly one executable `Start` function.

### 1.2 Types, values, and conversion

- [x] Define compatibility, assignment, equality, and comparison rules for all
  primitive, vector, struct, class, interface, pointer, and alternative types.
- [x] Define numeric promotion, widening, narrowing, overflow, division result
  type, exponentiation types, and conversion of `NAN` or infinity to integers.
- [x] Define left-to-right evaluation order for operands and arguments.
- [x] Define boolean short-circuit behavior for `AND` and `OR`, and evaluation
  rules for `XOR` and all bitwise operators.
- [x] Define struct copy depth, class-reference assignment, vector assignment,
  and `CONST` behavior for mutable referenced values.

### 1.3 Alternative values and flow

- [x] Define normalized alternative types: order, duplicates, and three-or-more
  alternatives.
- [x] Define `IS` for `NULL`, `NA`, `EOF`, `Error`, and named object types.
- [x] Define narrowing in `THEN` and `ELSE`, and when reassignment invalidates
  a narrowing fact.
- [x] Require a compatible `RETURN` on every completed path of a non-`VOID`
  `FUNCTION`.
- [x] Specify control-flow analysis for `IF`, loops, `RETURN`, and `EXIT`.
- [x] Define unreachable-code policy and diagnostics after `RETURN` or `EXIT`.

### 1.4 Diagnostics and standard surface

- [x] Create `docs/language/diagnostics.md` with diagnostic structure, source
  spans, severity, stable identifiers, and teacher-style wording rules.
- [x] Define errors for invalid syntax, names, types, missing returns, invalid
  `EXIT`, conversions, overflow, and invalid alternative-value use.
- [x] Define the `Math` contract in `docs/library/math.md`; keep language syntax
  in `0.1.md`.
- [x] Move `Console` and `Error` contracts into separate standard-library
  documentation; keep language syntax in `0.1.md`.
- [x] Define `PRINT`, `INPUT`, `EOF`, `Error`, `TryParse`, and `Parse` as
  normative standard-library/runtime contracts.
- [x] Define shared diagnostic format and exit codes for `bn check`, `bn run`,
  and `bn build`.

**Done when:** every valid core AST has one deterministic type and execution
meaning, and every invalid core AST has a defined diagnostic.

## Sprint 2 — Rust lexer

**Goal:** turn source bytes into a fully spanned, conformant token stream.

- [x] Create the Rust `bn` crate and source-location primitives.
- [x] Implement a handwritten linear UTF-8 scanner; do not use a parser
  generator for lexing.
- [x] Preserve `NEWLINE` tokens, update positions through block comments, and
  insert the specified final newline behavior.
- [x] Implement keywords, identifiers, literal forms, strings, comments, and
  maximal-munch operators from Sprint 0.
- [x] Reuse the positive and negative lexical fixtures as lexer conformance
  tests, including exact token streams and representative source spans.
- [x] Render lexical diagnostics with file, line, column, source excerpt, and
  a teacher-style explanation.

**Done when:** every Sprint 0 lexical fixture produces the expected token stream
or the expected diagnostic with a source span.

## Sprint 3 — Grammar analyzer (parser) and syntax AST

**Goal:** implement the grammar analyzer that parses every valid 0.1 source
program into a source-spanned syntax AST and rejects invalid structure.

**Current sprint status:** completed. The handwritten recursive-descent parser
builds a source-spanned syntax AST, delegates expressions to a Pratt parser,
and is covered by the accepted positive and syntax-negative fixtures.

- [x] Implement recursive descent for modules, declarations, blocks, statements,
  type references, and assignment targets.
- [x] Connect the existing Pratt parser to every expression-bearing grammar
  production and complete its remaining primary forms.
- [x] Represent `ELSE IF` as branches of one `IF` node.
- [x] Build AST nodes for modules, imports, declarations, statements,
  expressions, and type references; every node carries a `Span`.
- [x] Keep source names in the syntax AST; do not create a second IR yet.
- [x] Use `enum` and `match` for the first Rust AST implementation.
- [x] Diagnose wrong terminators using a block stack and synchronize at the
  grammar points defined in Sprint 0.
- [x] Add parser conformance tests for all positive fixtures and representative
  syntax-negative grammar fixtures.

**Done when:** the parser accepts every conforming example, rejects every
negative grammar fixture, and preserves precise source spans in the AST.

## Sprint 4 — Semantic analyzer

**Goal:** resolve names, validate types and flow, and produce an executable AST
without interpreting invalid source.

**Current sprint status:** core completed; extended semantics reopened by the
2026-08-22 frontend audit. The remaining items feed Sprint 7.

- [x] Build lexical scopes, symbol tables, `SymbolId`, and `TypeId`.
- [ ] Resolve imported module graphs and finish exports, constructors, private
  access, statics, and runtime-facing member resolution for classes, structs,
  interfaces, fields, methods, and visibility.
- [x] Normalize aliases (`INTEGER`, `FLOAT`) and alternative types.
- [ ] Finish validation of constructor calls, pointer shapes/lifecycles, and
  extended member calls. Core assignment, conversion, operations, calls,
  vectors, and interface signatures are validated.
- [x] Implement alternative-type narrowing and invalidation rules.
- [x] Implement structural return-path and unreachable-code analysis.
- [x] Validate `EXIT` and `CONTINUE` against the enclosing loop stack.
- [ ] Produce the remaining extended Sprint 1 semantic diagnostics before execution.

**Done when:** every semantic fixture yields either a validated program with
resolved symbols/types or the specified diagnostic.

## Sprint 5 — Typed IR and core interpreter

**Goal:** lower validated Basic Next programs to a typed control-flow IR and
safely execute the minimum useful subset through its Rust interpreter.

**Current sprint status:** completed. `--emit ir` produces validated typed BN
IR, and `bn run` executes the minimum useful subset through that IR.

- [x] Define typed functions, values, instructions, basic blocks, and explicit
  terminators; preserve source spans for runtime diagnostics.
- [x] Lower the validated AST to BN IR and implement `--emit ir`.
- [x] Validate generated IR before execution.
- [x] Implement function frames, lexical environments keyed by `SymbolId`, and
  deterministic left-to-right evaluation.
- [x] Implement primitive values, conversions, checked integer arithmetic,
  IEEE floats, Euclidean modulo, logical/bitwise operations, and shifts.
- [x] Implement `LET`, `CONST`, assignments, expressions, `IF`, `WHILE`,
  `REPEAT`, `FOR`, `FOR EACH`, `RETURN`, `EXIT`, and `CONTINUE`.
- [x] Model internal control flow as normal continuation, return value, loop
  exit, and loop continuation; do not use Rust panics for BN program control flow.
- [x] Implement vectors, `PRINT`, `INPUT`, `EOF`, `Error`, and the minimal
  Console/Math contracts.
- [x] Implement zero-config `bn check file.bn` and `bn run file.bn`.

**Done when:** `bn run examples/hello.bn` and the core conformance suite run
with the documented results and diagnostics.

## Sprint 6 — Object, memory, module, and host semantics

**Goal:** close the semantics that carry the largest runtime and safety risk
before implementing the extended runtime.

**Current sprint status:** completed at specification level. The accepted rules
are recorded in `docs/language/0.1.md` and indexed in
`docs/project/decisions-0.1.md`; implementation is decomposed across Sprints
7–12.

### 6.1 Classes, structs, interfaces, and statics

- [x] Define field initialization order, default values, constructor execution,
  constructor failure, and class reference semantics.
- [x] Define method lookup, interface dispatch, compatible interface signatures,
  and behavior of static members through class names.
- [x] Define static initialization order across modules and diagnostics for
  circular initialization.
- [x] Define struct field initialization, copy semantics for nested values, and
  parameter-passing semantics.

### 6.2 Heap and pointers

- [x] Define pointer default state, aliasing, base-versus-offset pointers, zero
  lengths, bounds checks, allocation failure, and runtime-sized regions.
- [x] Define `NEW` and `DELETE` grammar/semantics for classes and typed memory.
- [x] Define use-after-delete, double-delete, deletion during destruction, and
  behavior of aliases after deletion.
- [x] Define destructor timing, process-end behavior, undeleted allocations,
  cycles, and partially constructed objects.
- [x] State that the reference runtime uses checked allocation handles, never
  raw Rust pointers or `unsafe` memory access.

### 6.3 Host boundary

- [x] Define the base `HOST.main`, `SYSTEM`, and `HOST.clock` contracts.
- [x] Defer host-supplied memory until a concrete shared, mapped, device, or FFI
  contract exists.
- [x] Keep GPU, DOM, files, network, package registry, and other optional host
  capabilities outside the 0.1 interpreter milestone.

**Done when:** object allocation, invocation, static initialization, pointers,
deletion, modules, and host access can be implemented without runtime policy
decisions.

## Execution order for the remaining 0.1 work

```text
Sprint 7 semantic closure
→ Sprint 8 extended BN IR
→ Sprint 9 module/static runtime
→ Sprint 10 object/value runtime
→ Sprint 11 checked memory and lifecycle
→ Sprint 12 HOST and temporal runtime
→ Sprint 13 conformance and release readiness
```

Each sprint must leave `cargo fmt --check`, `cargo test`,
`cargo clippy --all-targets -- -D warnings`, and `git diff --check` passing.
No later sprint may compensate for unresolved types or names from an earlier
stage.

## Sprint 7 — Semantic closure and module graph

**Status:** in progress.

**Goal:** finish the reopened Sprint 4 work so every accepted executable
expression has a resolved type, symbol, member, and module identity before IR
lowering.

### Deliverables

- [x] Build the project-root module loader and an acyclic `ModuleId` graph from
  logical imports such as `Basic.Collections` → `Basic/Collections.bn`.
- [x] Resolve import aliases and exported declarations across modules; reject
  missing modules, cycles, private module declarations, duplicate aliases, and
  imported `Start` functions with source-spanned diagnostics.
- [ ] Resolve constructor calls, instance/static members, private access,
  struct fields, interface dispatch targets, and class/interface assignments.
- [ ] Validate `NEW`, pointer element shape, fixed/dynamic length compatibility,
  legal `DELETE` targets, and pointer alternative types before lowering.
- [ ] Give `DATE`, `TIME`, `TIMEZONE`, `HOST.main`, `HOST.clock`, and every
  accepted temporal/host member an exact semantic type; eliminate permissive
  `Unknown` results from executable accepted expressions.
- [ ] Add positive and negative semantic fixtures for every item above and
  stable diagnostic identifiers for module/member/constructor failures.

### Acceptance evidence

- `bn check` validates a multi-module program and rejects an import cycle.
- `--emit typed-ast` shows resolved symbols/types for constructor, member,
  pointer, temporal, host, and imported calls.
- A test asserts that no executable expression in the accepted full-frontend
  fixture has `Type::Unknown`.

**Done when:** the semantic model alone is sufficient to lower every accepted
0.1 program without name lookup or policy decisions in the IR stage.

## Sprint 8 — Extended typed BN IR

**Goal:** represent every accepted 0.1 operation in validated BN IR before
adding its runtime behavior.

### Deliverables

- [ ] Add IR value forms for structs, object handles, interface references,
  pointers, `DATE`, `TIME`, `TIMEZONE`, and imported capabilities.
- [ ] Add explicit instructions for field/member access, method/interface
  dispatch, constructors, statics, allocation, indexing, deletion,
  destructors, module access, host calls, and temporal calls.
- [ ] Preserve resolved `SymbolId`, type, module identity, dispatch target, and
  source span in lowering; runtime lookup by source-level name is forbidden.
- [ ] Extend IR validation for value definitions, block targets, ownership
  operands, member targets, call signatures, and terminators.
- [ ] Make `--emit ir examples/language-tour.bn` succeed and add focused IR
  regression tests for each new instruction family.

### Acceptance evidence

- The complete frontend fixture lowers to IR and passes structural validation.
- Deliberately malformed IR is rejected before execution with `INVALID_IR`.

**Done when:** remaining runtime sprints only execute validated instructions;
they do not inspect the syntax AST or invent missing semantic information.

## Sprint 9 — Modules and static storage runtime

**Goal:** execute a deterministic multi-module program and initialize static
state according to the accepted specification.

### Deliverables

- [ ] Load the complete resolved module graph once and identify exactly one
  executable `Start`.
- [ ] Execute exported function calls across modules without exposing private
  declarations.
- [ ] Allocate per-module and per-class static storage and initialize fields in
  source order on first class use.
- [ ] Detect recursive static initialization with
  `STATIC_INITIALIZATION_CYCLE`; never expose partially initialized values.
- [ ] Add multi-module fixtures for forward references, private exports,
  missing modules, cycles, static ordering, and static cycles.

### Acceptance evidence

- One positive multi-module program executes with documented output.
- Every module/static negative fixture produces its documented diagnostic.

**Done when:** modules and static state no longer require special cases in the
object runtime.

## Sprint 10 — Struct, class, and interface runtime

**Goal:** execute the complete accepted object/value model.

### Deliverables

- [ ] Implement struct defaults, nested deep-copy assignment/parameters,
  structural equality, field access, and vectors of structs.
- [ ] Implement class allocation, field initialization, constructor calls,
  reference assignment, identity equality, and instance/static methods.
- [ ] Implement `SELF`, private access already authorized by semantics, exact
  interface dispatch, and vectors of class/interface references.
- [ ] Ensure failed construction exposes no object and schedules no destructor.
- [ ] Add conformance fixtures distinguishing struct copies from class aliases
  and direct calls from interface dispatch.

### Acceptance evidence

- Object and aggregate examples run through BN IR without AST interpretation.
- Copy, identity, visibility, constructor, and dispatch tests pass.

**Done when:** all accepted OO behavior except explicit deletion/destruction is
executable and deterministic.

## Sprint 11 — Checked memory and lifecycle

**Goal:** integrate the existing generational heap foundation with pointers,
objects, `DELETE`, and destructors without Rust `unsafe`.

### Deliverables

- [ ] Integrate heap slot/generation handles into runtime values; retain the
  existing bounds, stale-handle, slot-reuse, and zero-length tests.
- [ ] Implement numeric `NEW TYPE[count]`, fixed/dynamic pointer views,
  initialization, checked indexing, alias copying, and permitted base deletion.
- [ ] Implement class `DELETE`, exactly-once destructors, deletion during
  destruction, constructor failure cleanup, and process-end recovery without
  destructor execution.
- [ ] Produce `NULL_POINTER_ACCESS`, `INDEX_OUT_OF_BOUNDS`,
  `USE_AFTER_DELETE`, `DOUBLE_DELETE`, and allocation-size diagnostics at the
  original BN source span.
- [ ] Add lifecycle fixtures for aliases, stale generations, zero lengths,
  double/reentrant delete, leaked allocations, and failed construction.

### Acceptance evidence

- Pointer/object lifecycle fixtures run without raw pointers or `unsafe`.
- Miri is optional evidence; the portable release gate remains the checked
  runtime and conformance suite.

**Done when:** every accepted allocation and destruction path has deterministic
ownership behavior and a checked failure mode.

## Sprint 12 — Base HOST and temporal runtime

**Goal:** implement the accepted environment boundary and temporal value model
without making deterministic operations depend implicitly on host state.

### Deliverables

- [ ] Bind `HOST.main` to immutable process arguments; `ArgumentCount()`
  includes the executable and `Argument(0)` returns its host-supplied spelling.
- [ ] Bind `HOST.clock`: `Timestamp()` returns Unix-epoch milliseconds and
  `Monotonic()` returns nanoseconds from an unspecified monotonic origin.
- [ ] Implement `DATE`, `TIME`, and `TIMEZONE` runtime values, defaults,
  equality, ordering where specified, canonical `PRINT`, and vectors.
- [ ] Implement strict RFC 3339 timestamp parsing/formatting, four-digit years,
  ISO date/time parsing, UTC normalization, precision rejection, and range
  diagnostics.
- [ ] Implement `Math.TODATE`, `Math.TOTIME`, `Math.TOTIMESTAMP`, `TOHOUR`, and
  `TOWEEKDAY`, including negative pre-epoch timestamps.
- [ ] Select and document the IANA TZDB source/version strategy before adding a
  dependency; expose the version used by time-zone-dependent results.
- [ ] Diagnose unavailable imported capabilities before `Start` with
  `HOST_CAPABILITY_UNAVAILABLE`.

### Acceptance evidence

- Argument fixtures verify entry `0`, empty user arguments, Unicode, and
  ordering.
- RFC 3339 vectors cover `Z`, offsets, epoch boundaries, leap years, invalid
  dates, excess precision, `24:00`, leap seconds, and years outside
  `0001..9999`.
- Clock tests inject deterministic providers; wall-clock tests never depend on
  the machine's current time.

**Done when:** portable host behavior and every accepted temporal conversion
are executable, testable, and reproducible except for explicitly acquired
clock values.

## Sprint 13 — Conformance and release readiness

**Goal:** turn the completed implementation into a reproducible 0.1 release
candidate. Tagging or publishing still requires an explicit maintainer request.

### Deliverables

- [ ] Convert every accepted example and stable diagnostic into an
  implementation-independent conformance case with expected output or code.
- [ ] Make `examples/language-tour.bn` execute rather than serve only as a
  frontend fixture; replace or remove the placeholder `shortest_path.bn`.
- [ ] Add concise installation, command, limitation, and troubleshooting
  documentation plus release notes and a `v0.1.0` checklist.
- [ ] Finish the program style guide and paradigm-boundary notes that do not
  block implementation but belong in the published language documentation.
- [ ] Verify a clean clone builds, checks every example, runs conformance, and
  reproduces documented CLI output without untracked generated artifacts.
- [ ] Record exact Rust/tool versions and the selected TZDB release used for
  release evidence.

### Acceptance evidence

- One documented command performs the complete clean-clone release check.
- `bn check`, `bn run`, diagnostics, examples, and conformance output agree
  with the normative documents.

**Done when:** Basic Next 0.1 is ready for an explicitly authorized tag and
publication, with a conformance suite another implementation can reuse.

## Post-0.1 — Compiler track

**Goal:** add compilation without creating a second language implementation.
The compiler must reuse the lexer, grammar analyzer, AST, semantic analyzer,
diagnostics, and conformance suite used by the interpreter.

- [ ] Define native-code and WebAssembly artifact contracts, supported host
  capabilities, and portability requirements.
- [ ] Lower typed BN IR to LLVM IR; do not lower directly from tokens, source
  text, or the syntax AST.
- [ ] Reuse the same name resolution, type checking, overflow rules, and
  capability checks as the interpreter.
- [ ] Implement a compiler driver and an explicit `bn build` artifact contract.
- [ ] Add LLVM native-code and WebAssembly generation.
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
| Extended name/type/module resolution | Sprint 7 |
| Full accepted-language IR | Sprint 8 |
| Imports, exports, modules, and static initialization | Sprint 9 |
| Structs, classes, constructors, methods, and interfaces | Sprint 10 |
| Checked pointers, heap lifecycle, `DELETE`, and destructors | Sprint 11 |
| Base `HOST`, RFC 3339, UTC conversion, and IANA TZDB | Sprint 12 |
| Full conformance, documentation, and release readiness | Sprint 13 |
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
