# Basic Next Workspace Crates

This directory contains the modular crates that form the Basic Next toolchain workspace.
The architecture enforces a single pipeline:

`Source -> Frontend -> Lowering -> BN IR -> validate -> interpret(IR) | compile(IR)`

## Crates Overview

### Core & Pipeline

- **`bn_source`**: Source text identity, spans, file references, and line/column resolution.
- **`bn_diag`**: Diagnostic models, error/warning codes, spans, and diagnostic sinks.
- **`bn_types`**: Fundamental type system definitions (`Type`, `IntegerType`, `FloatType`), sizing rules, and scalar byte widths.
- **`bn_value`**: Runtime dynamic value representations (`Value`) used across interpreter and runtime evaluation.
- **`bn_ir`**: Typed intermediate representation model (BN IR), SSA control-flow graphs, basic blocks, instructions, and target-independent IR validation (`validate`).
- **`bn_frontend`**: Shared frontend sessions, lexical/parsing abstractions, and AST-to-IR lowering.

### Backends & Execution

- **`bn_runtime`**: Direct IR interpreter execution engine and heap allocation.
- **`bn_interp`**: Interpreter runtime coordination and evaluation loop.
- **`bn_rt`**: Native runtime library (`libbn_rt.a`), C ABI boundary, execution policy checks, and host runtime providers.
- **`bn_llvm`**: LLVM code-generation backend compiling validated BN IR into native binaries or WebAssembly.

### Standard Library & HOST Providers

- **`bn_host_exec`**: Host process execution capabilities (`HOST.Exec`).
- **`bn_host_fs`**: Host filesystem access and sandboxing policy (`HOST.FileSystem`).
- **`bn_host_net`**: Host network sockets and addressing (`HOST.Net`).
- **`bn_lib_crypto`**: Cryptographic functions and hashing (`BNCrypto`).
- **`bn_lib_data`**: Data manipulation, DataFrame structures, and CSV parsing (`BNData`).
- **`bn_lib_dispatch`**: Parallel and asynchronous task dispatch (`BNDispatch`).
- **`bn_lib_json`**: JSON parsing and serialization (`BNJson`).
- **`bn_lib_log`**: Structured logging transports and formatters (`BNLog`).
- **`bn_lib_math`**: Mathematical functions and constants (`BNMath`).
- **`bn_lib_web`**: HTTP client and web utilities (`BNWeb`).
- **`bn_limits`**: Resource limits, bounds, and execution policy thresholds.

### Toolchain Drivers & Binaries

- **`bn_cli`**: Shared CLI option parsing, environment resolution, and driver interfaces.
- **`bn_interpret_driver`**: Driver library for the interpreter and frontend tools.
- **`bn_compile_driver`**: Driver library for native compilation.
- **`bn_lsp`**: Language Server Protocol (LSP) server implementation.
- **`bn_dap`**: Debug Adapter Protocol (DAP) server implementation.
- **`bni`**: Unified interpreter and frontend CLI executable (`run`, `eval`, `check`, `lex`, `lsp`, `dap`).
- **`bnc`**: Compiler CLI executable producing native binaries or Wasm artifacts.

## Architectural Rules

- **Specification Above Implementations**: Crates adhere to `docs/architecture/` contracts and active language specifications (`language/0.6/`).
- **Single Validated IR**: Both the interpreter (`bn_runtime`) and compiler (`bn_llvm`) consume the exact same validated IR produced by `bn_ir::validate`.
- **No Cyclic Dependencies**: Dependencies between crates follow a strict acyclic directed graph governed by `scripts/check-forbidden-deps.sh`.
