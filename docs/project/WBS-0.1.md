# WBS — Basic Next 0.1

This WBS applies a lightweight PMI approach: it organizes scope by deliverable,
not by people or dates. Each level-2 item is a GitHub Project card.

## Delivery objective

Publish Basic Next 0.1 with a reviewed specification, executable examples, and
a reference implementation able to validate and execute programs in the 0.1
scope. Native compilation, GUI, concurrency, and AI integrations are outside
this delivery.

## Acceptance criteria

- The 0.1 specification defines syntax and semantics without depending on a
  virtual machine.
- Official examples execute in the reference implementation.
- Lexical, syntax, and type errors produce understandable diagnostics.
- The repository explains how to run examples and reproduce the conformance
  suite.
- A `v0.1.0` release is published with release notes.

## Work breakdown structure

### 1. Delivery management

- **1.1 Scope and acceptance criteria:** freeze 0.1 content and record what
  remains outside it.
- **1.2 Delivery control:** maintain the Kanban board, decisions, and release
  notes.

### 2. Language specification

- **2.1 Lexical grammar:** reserved words, tokens, comments, strings, and EBNF.
- **2.2 Types and expressions:** primitive types, operators, precedence,
  assignment, and allowed conversions.
- **2.3 Declarations and control flow:** variables, `SUB`, `FUNCTION`, `CLASS`,
  `IF`, `WHILE`, `REPEAT`, `FOR`, and `RETURN`.
- **2.4 Modules and environment:** `IMPORT`, executable module, `SUB Start()`,
  and the `HOST.main` contract.
- **2.5 Diagnostics and conformance:** defined errors and normative examples.

### 3. Reference implementation

- **3.1 Architecture gate:** choose an execution engine without changing the
  language semantics (AST interpreter, own VM, or Wasm/WAMR).
- **3.2 Front end:** lexer, parser, and AST for the complete 0.1 grammar.
- **3.3 Semantic analysis:** scope, name resolution, types, and diagnostics.
- **3.4 Execution:** execute 0.1 programs on the engine approved in 3.1.
- **3.5 BN command-line tool:** minimum `BN` commands to check and execute
  Basic Next files.

### 4. Quality and publication

- **4.1 Conformance:** suite of examples and error cases.
- **4.2 Usage documentation:** installation, first program, and contribution.
- **4.3 Release 0.1.0:** versioning, notes, and artifact publication.

## Main dependencies

```text
2.1–2.5 ──→ 3.2 ──→ 3.3 ──→ 3.4 ──→ 4.1 ──→ 4.3
                 ↑
                3.1
```

Item 3.1 is a gate: it blocks execution implementation, not specification
progress.
