# Basic Next Workspace Crates

This directory contains the modular crates that form the Basic Next toolchain workspace.
The architecture enforces a single pipeline:

`Source -> Frontend -> Lowering -> BN IR -> validate -> interpret(IR) | compile(IR)`

## Crates Overview

- **`bn_source`**: Source text identity, spans, file references, and line/column resolution.
- **`bn_diag`**: Diagnostic models, error/warning codes, spans, and diagnostic sinks.
- **`bn_types`**: Fundamental type system definitions (`Type`, `IntegerType`, `FloatType`), sizing rules, and scalar byte widths.
- **`bn_value`**: Runtime dynamic value representations (`Value`) used across interpreter and runtime evaluation.
- **`bn_ir`**: Typed intermediate representation model (BN IR), SSA control-flow graphs, basic blocks, instructions, and target-independent IR validation (`validate`).
- **`bn_frontend`**: Shared frontend sessions, lexical/parsing abstractions, and AST-to-IR lowering.
- **`bn_runtime`**: Execution engine and heap allocation for direct IR interpretation.
- **`bn_rt`**: Native runtime library, C ABI boundary, execution policy checks, and host runtime providers.
- **`bn_llvm`**: LLVM code-generation backend compiling validated BN IR into native binaries or WebAssembly.

## Architectural Rules

- **Specification Above Implementations**: Crates adhere to `docs/architecture/` contracts and active language specifications (`docs/language/0.4/`).
- **Single Validated IR**: Both the interpreter (`bn_runtime`) and compiler (`bn_llvm`) consume the exact same validated IR produced by `bn_ir::validate`.
- **No Cyclic Dependencies**: Dependencies between crates follow a strict acyclic directed graph governed by `scripts/check-forbidden-deps.sh`.
