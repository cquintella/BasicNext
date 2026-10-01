# Workspace Root Package (`src/`)

This directory contains the root package entry point (`src/lib.rs`) for the Cargo workspace.

## Purpose

The workspace root package (`bn`) is an empty library crate whose primary purpose is to anchor the workspace root and host the cross-crate integration tests located in `tests/` (e.g., parity testing, IR validation, code generation, CLI behavior).

## Modular Toolchain Architecture

The monolithic compiler implementation that previously resided under `src/` was migrated to modular crates under `crates/`.

- **Executables**:
  - `bni` (`crates/bni`): Interpreter, evaluation, check, LSP, and DAP driver.
  - `bnc` (`crates/bnc`): Clang-like native and WebAssembly LLVM compiler driver.
- **Shared Crates**: All lexical analysis, parsing, semantic checking, IR lowering, validation, runtime execution, and standard modules live in dedicated crates under `crates/`.
- **Integration Tests**: Workspace-wide tests live under `tests/` and test the shared crates and drivers directly.

Refer to [`crates/README.md`](../crates/README.md) for the complete crates inventory and [`docs/architecture/`](../docs/architecture/) for detailed architectural specifications.
