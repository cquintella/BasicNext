# WBS — Basic Next 0.1

This WBS applies a lightweight PMI approach: it organizes scope by deliverable,
not by people or dates. Each level-2 item is a GitHub Project card.

## Scope baseline

**Status:** Accepted for 0.1 planning on 2026-08-18.

**Authority:** the maintainer-approved language documents and this WBS.

Accepted language decisions are indexed in
[`decisions-0.1.md`](decisions-0.1.md); the normative language documents remain
authoritative.

### In scope for 0.1

- The normative EBNF and lexical contract for the complete accepted 0.1
  syntax.
- Static semantics: names, scopes, explicit types, conversions, alternatives,
  control-flow validity, diagnostics, and return-path analysis.
- A Rust reference pipeline consisting of lexical analyzer, grammar analyzer,
  syntax AST, semantic analyzer, and tree-walk interpreter.
- The minimum useful program subset with `bn check` and `bn run` as the
  zero-configuration command contract.
- Classes, structs, interfaces, statics, modules, checked pointers, `NEW`,
  `DELETE`, and the defined base `HOST` contract, subject to their semantic
  safety rules.
- Positive and negative conformance fixtures, examples, diagnostics, usage
  documentation, and a reproducible `v0.1.0` release.

### Explicitly outside 0.1

- Native-code and WebAssembly compilation. They are tracked in the post-0.1
  compiler workstream and must reuse the validated front end.
- VM, JIT, async/await, general concurrency, and executable `PARALLEL` syntax.
- GPU, DOM, filesystem, network, package registry, and optional host
  capabilities beyond the defined base contract.
- C FFI, generic classes, inheritance, reflection, variable-size collections,
  DataFrame support, and editor/LSP tooling.

## Delivery objective

Publish Basic Next 0.1 with a reviewed specification, executable examples, and
a reference implementation able to validate and execute programs in the 0.1
scope. Compilation, GUI, concurrency, and AI integrations are outside this
delivery; compilation is tracked explicitly as a post-0.1 workstream.

## Release acceptance criteria

- **Specification:** `0.1.ebnf`, `0.1.md`, and `keywords.md` agree; every
  accepted syntax rule has defined semantics and every excluded feature is
  explicitly outside 0.1.
- **Lexical analyzer:** every lexical fixture produces the documented tokens or
  a source-spanned diagnostic.
- **Grammar analyzer and AST:** every valid fixture parses into a source-spanned
  AST and every invalid fixture is rejected without inventing syntax.
- **Semantic analyzer:** valid programs receive resolved symbols and types;
  invalid programs receive the documented static diagnostic before execution.
- **Interpreter:** `bn run` executes the minimum useful subset and the approved
  extended 0.1 semantics without unchecked memory access or Rust panic-driven
  BN control flow.
- **Diagnostics:** `bn check` and `bn run` use the same source locations,
  diagnostic structure, and exit-code model.
- **Conformance:** official examples and positive/negative fixtures are
  reproducible from a clean clone.
- **Release:** the repository documents installation, usage, limitations, and
  publishes a versioned `v0.1.0` release.

## Work breakdown structure

### 1. Delivery management

- **1.1 Scope and acceptance criteria — Complete:** the 0.1 baseline and release
  gates are frozen above; implementation evidence is still pending in later
  work packages.
- **1.2 Delivery control — Active:** maintain the Kanban board, decisions, and release
  notes.

### 2. Language specification

- **2.1 Normative EBNF:** build and maintain `docs/language/0.1.ebnf` as the
  machine-readable grammar, including blocks, declarations, statements, types,
  and expressions.
- **2.2 Lexical contract:** reserved words, tokens, comments, strings, numeric
  literals, source spans, and lexical errors.
- **2.3 Types and expressions:** primitive types, operators, precedence,
  assignment, and allowed conversions.
- **2.4 Declarations and control flow:** variables, `FUNCTION`, `CLASS`,
  `STRUCT`, `STATIC`, `IF`, `WHILE`, `REPEAT`, `FOR`, `FOR EACH`, `EXIT`, and
  `RETURN`.
- **2.5 Modules and environment:** `IMPORT`, executable module,
  `FUNCTION Start() AS VOID`,
  and the `HOST.main` contract.
- **2.6 Diagnostics and conformance:** defined errors and normative examples.

### 3. Reference implementation

- **3.1 Lexical analyzer:** implement the UTF-8 scanner, tokenization, source
  spans, comments, literals, and maximal-munch operators.
- **3.2 Grammar analyzer:** implement the recursive-descent/Pratt parser for
  the complete 0.1 EBNF and produce syntax diagnostics.
- **3.3 Syntax AST:** represent modules, declarations, statements, expressions,
  and type references with source spans.
- **3.4 Semantic analyzer:** resolve scopes and names, validate types and flow,
  and produce the validated AST used by execution.
- **3.5 Rust interpreter:** execute the validated AST with the tree-walk
  reference runtime. A VM and JIT are outside 0.1.
- **3.6 BN command-line tool:** minimum `BN` commands to check and execute
  Basic Next files.

### 4. Quality and publication

- **4.1 Conformance:** suite of examples and error cases.
- **4.2 Usage documentation:** installation, first program, and contribution.
- **4.3 Release 0.1.0:** versioning, notes, and artifact publication.

## Main dependencies

```text
2.1–2.6 → 3.1 lexical analyzer → 3.2 grammar analyzer → 3.3 AST
→ 3.4 semantic analyzer → 3.5 interpreter → 4.1 conformance → 4.3 release
```

The Rust tree-walk interpreter is the 0.1 execution baseline. Later compiler
backends must preserve the language semantics and conformance suite.

## Effort and deliverable baseline

These are preliminary planning estimates in person-days for one experienced
contributor. They exclude external approvals, tooling outages, and unsolved
language decisions. They are ranges, not commitments; actual effort is
recorded when each work package is completed.

| Work package | Deliverable | Acceptance evidence | Depends on | Estimate |
| --- | --- | --- | --- | ---: |
| Scope and acceptance | Approved 0.1 baseline in this WBS | Maintainer-approved in/out scope and release gates | — | 1–2 |
| Normative EBNF | `0.1.ebnf` and grammar fixtures | Grammar review finds no undefined or conflicting production | Scope | 3–5 |
| Lexical analyzer | Rust scanner, tokens, spans, lexical diagnostics | All lexical fixtures produce expected tokens/errors | EBNF | 3–5 |
| Grammar analyzer and AST | Parser plus source-spanned syntax AST | Valid fixtures parse; invalid fixtures are rejected | EBNF, lexer | 5–8 |
| Semantic analyzer | Symbols, types, flow checks, validated AST | Semantic fixtures resolve or emit documented diagnostics | AST | 8–12 |
| Interpreter | Rust tree-walk runtime for 0.1 | Approved examples execute with documented results | Semantic analyzer | 8–15 |
| Objects, memory, modules, HOST | Safe lifecycle and capability runtime | Lifecycle, pointer, module, and host tests pass | Interpreter | 10–20 |
| Conformance and release | Reproducible suite, docs, `v0.1.0` | Clean clone reproduces checks and examples | All 0.1 work | 5–8 |
| Compiler (post-0.1) | IR, native and WebAssembly backends, `bn build`, parity suite | Compiled and interpreted results agree | Validated front end | TBD |

## 5. Post-0.1 compiler workstream

- **5.1 Compiler contract:** define native-code and WebAssembly `bn build`
  artifacts, inputs, outputs, diagnostics, and portability guarantees.
- **5.2 Compiler IR:** lower the validated AST into a small target-independent
  intermediate representation.
- **5.3 Backends:** generate native code and WebAssembly, and link the required
  runtime capabilities.
- **5.4 Parity:** run the same conformance suite against interpreted and
  compiled programs and document intentional target limitations.
