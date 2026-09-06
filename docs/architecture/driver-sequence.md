# Current driver sequence (0.4.4)

This is an as-is inventory for the 0.4.4 soft-preparation bucket. It records
the code that runs today; the approved target remains one shared
frontend → IR → `validate` → interpret/compile pipeline.

## CLI (`bn run` and `bn build`)

The root dispatcher is `src/main.rs`; frontend loading is in
`src/cli_frontend.rs`.

| Phase | Current implementation | `run` | `build` |
| --- | --- | :---: | :---: |
| Read source | `fs::read_to_string` + `SourceFile::new` in `main` | yes | yes |
| Lex | `lexer::lex` in `main` | yes | yes |
| Load module graph | `cli_frontend::load_frontend` → `module_graph::load` | yes | yes |
| Parse root | `parse_named` in `load_frontend` | yes | yes |
| Semantic analysis | `analyze_modules` in `load_frontend` | yes | yes |
| Lower and language validate | `ir::lower_graph` (which calls `ir::validate`) | yes | yes |
| Backend | `runtime::execute_with_host` | interpret | `llvm::lower_module_for_target`, then clang/wasm-ld |

Both commands call `lower_graph` once for their backend path. `bn check`
performs lexical, syntax, and semantic checks; it lowers only when
`--emit ir` is requested, so it is not currently a full IR-validation check.
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
[bucket](../../ongoing/bucket-0.4.4.md).
