# Historical driver sequence (0.4.4)

**2026-09-21:** [Library-first drivers](library-first-drivers.md) defines current
ownership and migration. In the inspected pre-0.6.0 checkout, CLI preparation
uses lower_graph_validated, compilation uses validate_for, and LSP has graph
diagnostics. Old limitations below are historical. DAP still prepares in both
validate_launch and execute_program; reuse must respect source revisions.

This is an as-is inventory for the 0.4.4 soft-preparation bucket. It records
the code that runs today; the approved target remains one shared
frontend → IR → `validate` → interpret/compile pipeline.

## CLI (`bni run` and `bnc`)

Since bucket 0.6.0 the executables are `crates/bni/src/main.rs` and
`crates/bnc/src/main.rs`; both are dispatch only. The shared phases are in
`bn_cli` and `bn_frontend`; the backend phases in the two driver crates.

| Phase | Implementation (0.6.0) | `bni run` | `bnc` |
| --- | --- | :---: | :---: |
| Read source + lex | `bn_cli::frontend::read_source` | yes | yes |
| Load module graph, parse, analyze, lower, **language validate** | `bn_frontend::prepare::prepare` (one call, structured errors), adapted by `bn_cli::frontend::load_frontend` | yes | yes |
| Warning policy | `bn_cli::diagnostics::emit_frontend_warnings` | yes | yes |
| Target support | — | — | `bn_llvm::validate_for` in `bn_compile_driver::build` |
| Backend | `bn_interp::execute_validated_with_host` over `bn_interpret_driver::environment::host_env` | interpret | `bn_llvm::lower_validated_module_for_target_with_policy`, then clang/wasm-ld (`bn_compile_driver::artifact`) |

Both executables consume the same `Prepared` artifact (W1/W3); `bni check`
runs the full preparation including language validation and emits an
artifact only on `--emit`.
That is tracked for S1.1 and is not changed by S0.3.

## LSP

`src/lsp.rs` keeps documents in an in-memory map and, on open/change, lexes and
parses the document, then calls the single-file semantic analyzer. Completion,
definition, references, hover, and document-symbol handlers repeat bounded
lex/parse work for the relevant document(s). The current LSP path does not call
`module_graph::load`, `ir::lower_graph`, or `ir::validate`; this is a recorded
pre-`FrontendSession` divergence for 0.4.5 XM4/SM3 work.

## DAP

`src/dap.rs` has two related paths:

1. `validate_launch` loads the graph, analyzes it, and lowers it as a launch
   preflight.
2. `execute_program` loads and analyzes the graph again, lowers a new module,
   then calls `execute_with_host_debug_control` with the debug hook.

The duplicate load/analyze/lower is an as-is inefficiency and a future shared
session/store task. It does not create a second language implementation, but a
new DAP feature must not add another frontend or backend path.

## 0.4.4 policy

The CLI sequence above is the only documented driver sequence. New CLI, LSP,
or DAP work must reuse the existing frontend → IR boundary and must not add a
parallel parser, semantic analyzer, lowering path, or interpreter entrypoint.
The LSP and DAP divergences are inventory items, not permission to widen scope
in this bucket.

Related contracts: [target architecture](target-architecture.md),
[FrontendSession](frontend-session.md), and the 0.4.4 S0.3 activity in the
[bucket](../../done/bucket-0.4.4.md).
