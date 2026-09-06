# Current ownership inventory

This is the 0.4.4 soft-preparation inventory for the current single-package
tree. It records present ownership against the approved target split; it does
not move code or claim that crate boundaries already exist.

## Ownership map

| Area | Current paths | Responsibility | Target owner |
| --- | --- | --- | --- |
| Frontend | `src/lexer.rs`, `src/token.rs`, `src/parser.rs`, `src/parser/`, `src/ast.rs`, `src/source.rs`, `src/module_graph.rs`, `src/semantic.rs`, `src/semantic/`, `src/keyword_registry.rs` | Lexing, parsing, source identities, module graph, name/type analysis, keyword data | `bn_frontend`; `src/source.rs` becomes the shared `bn_source` leaf |
| Frontend lowering | `src/ir/lowering.rs`, `src/ir/lowering_callable.rs`, `src/ir/builder.rs`, `src/ir/builder/` | Converts analyzed AST/module graph into BN IR | `bn_frontend` internal lowering module |
| IR | `src/ir.rs`, `src/ir/model.rs`, `src/ir/validate.rs`, `src/ir/helpers.rs` | IR model, operations, identities, and language validation | `bn_ir`; current semantic-type imports are recorded debt |
| Interpreter/backend | `src/runtime.rs`, `src/runtime/`, `src/runtime_impl.rs`, `src/heap.rs`, `src/dataframe.rs` | Executes IR, owns heap/value behavior and runtime helpers | `bn_runtime` plus future `bn_value` leaf |
| HOST/network | `src/net.rs`, `src/net/`, `src/http.rs`, `src/tls.rs`, `src/web.rs`, `src/web/`, `src/web_state.rs`, `src/dispatch.rs`, `src/dispatch/`, `src/log.rs`, `crates/bn_rt/` | Host providers, transport, dispatch, logging, and native ABI | `bn_host_*`, `bn_runtime`, and `bn_rt` according to the target table |
| LLVM backend | `src/llvm.rs`, `src/llvm/` | Emits LLVM and lowers runtime calls | `bn_llvm`; must consume validated IR |
| Diagnostics | `src/diagnostic.rs` | Diagnostic data and rendering used across current modules | `bn_diag` shared leaf |
| CLI/edge | `src/main.rs`, `src/cli_frontend.rs`, `src/cli_help.rs`, `src/cli_output.rs`, `src/cli_toolchain.rs`, `src/config.rs` | Command parsing and pipeline orchestration | `bn` thin driver |
| LSP edge | `src/lsp.rs`, `src/lsp/` | LSP protocol and editor queries | `bn-lsp`; must use shared frontend/session |
| DAP edge | `src/dap.rs`, `src/dap/` | DAP protocol and debug execution control | `bn-dap`; must use shared frontend/IR/runtime |

## Current deviations to carry as debt

- The repository is still one Cargo package, so target crate boundaries are
  conceptual only.
- `src/ir/model.rs` currently imports semantic and module-graph types; this is
  the documented IR independence debt and must not grow.
- `src/ir/` contains both IR-owned validation and frontend-owned lowering;
  lowering moves with the frontend at the hard split.
- Runtime and edge modules still share the monolithic crate, so this inventory
  does not claim that dependency directions are already enforced.

## Review boundary

The approved target ownership remains in
[`target-architecture.md`](target-architecture.md). The milestone source for
this inventory is M0/A0.1 in
[`fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md).
The next 0.4.4 activity adds executable dependency enforcement; it must use
this inventory as its baseline rather than silently redefining ownership.
