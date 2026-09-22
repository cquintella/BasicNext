# Library-first drivers: bni and bnc

## Authority and implementation status

Carlos confirmed this direction on 2026-09-21: two executables consume shared
libraries; maximize library ownership and avoid duplicated implementation.
First extract the libraries while preserving existing commands, then change
`bnc`, then create `bni`. This supersedes the 2026-09-04 target in which `bnc`
was a controller that selected interpretation by default and spawned `bn`.

This is an approved architectural direction, not a completion claim. The
checkout still has `src/main.rs` as the combined driver and `src/bnc.rs` as a
wrapper. The new crate names below are the proposed implementation decomposition.
The implementation tracker is the local bucket 0.6.0 (not published);
its 2026-09-21 revision records library-first ownership and sequencing,
acceptance gates and the remaining CLI, compatibility and release decisions.

The command surface is `bni run|eval|check|lex|lsp|dap` and Clang-like
`bnc [compile-options] <entry.bn>` (`-o`, `--target`, `--opt`, `--emit ir`).
`bnc` has no build subcommand or `-c` alias. The 0.5 `bn` executable is
removed in 0.6.0 (Carlos, 2026-09-22; earlier plan: dispatcher through 0.6). No language, HOST catalog or IR extension is authorized by the
executable split.

## Ownership and dependencies

| Owner | Responsibility | Consumers |
| --- | --- | --- |
| Existing `bn_source`, `bn_diag`, `bn_types` | Source identity/revision, diagnostics and shared types | Frontend, IR and tools |
| Existing `bn_frontend` | Session, snapshots, module resolution, analysis and lowering; shared program preparation | CLI, LSP and DAP |
| Existing `bn_ir` | IR model and language validation | Both backends |
| New `bn_cli` | Common options/configuration, CLI adaptation of frontend preparation, check, diagnostic presentation, output and process log | Both drivers and executable command dispatch |
| New `bn_interpret_driver` | Run/eval integration, HostEnv construction, shipped provider composition and execution I/O | Existing bn during extraction, future bni, DAP |
| New `bn_compile_driver` | Build orchestration, target support, LLVM emission integration, tool discovery and artifact linking | Existing bn during extraction, future bnc |
| New `bn_lsp` | LSP transport, requests and conversion of frontend results | Existing bn, future bni |
| New `bn_dap` | DAP transport, lifecycle and debug control using the interpretation driver | Existing bn, future bni |
| Existing `bn_interp` | Language execution over validated IR | Interpretation driver |
| Existing `bn_llvm` | Target support and LLVM generation over validated IR | Compilation driver |
| Executables bni / bnc | Argument acquisition, command selection, library calls and exit status | Users and integrations |

`bn_cli` must not depend on `bn_interp`, `bn_llvm` or concrete providers.
`bn_lsp` must not depend on either backend. `bn_dap` may depend on the
interpretation driver; the interpretation driver must not depend on DAP.
Neither binary owns reusable application logic. Separate driver libraries
allow the existing executables to exercise the extracted implementation before
new entrypoints are introduced.

## Source-to-library migration map

Paths below describe the pre-extraction checkout. Module destinations are
proposed; they do not imply that a new crate already exists.

| Destination | Existing source and symbols | Extraction boundary |
| --- | --- | --- |
| `bn_cli::options` | `src/cli_frontend.rs::parse_options`; `src/main.rs::{Options, Color, OutputFormat, Emit}` | Common options only; split target/optimization and eval/run-specific fields into their drivers |
| `bn_cli::config` | `cli_frontend.rs::{config_path_from_arguments, config_path_from_context, ConfiguredSettings, configured_settings, parse_module_paths, strip_toml_comment, parse_module_path_item}` | One configuration discovery/parser and documented precedence |
| `bn_frontend` preparation service | Analysis/lowering body of `cli_frontend.rs::load_frontend_with_overlays`, existing `FrontendSession`, module graph and lowering APIs | Return structured results/errors; accept snapshots/overlays and module roots; no printing or CLI dependency |
| `bn_cli::frontend` | `main.rs::Frontend`; `cli_frontend.rs::{load_frontend, load_frontend_with_overlays}` | Adapt CLI options to the frontend service; retain validated artifact and source provenance |
| `bn_cli::check` | `main.rs::check` | One check implementation exposed publicly by bni only |
| `bn_cli::diagnostics` | `main.rs::{render_diagnostic, diagnostic_json, emit_frontend_warnings}`; `cli_frontend.rs::emit_frontend_error` | Shared presentation and warning policy; frontend service returns errors instead of printing |
| `bn_cli::output` | `src/cli_output.rs`; `main.rs::{language_error, tool_error, emit_output, tokens_text}` | Common output helpers and exit classification |
| `bn_cli::help` | `src/cli_help.rs` and wrapper help formatting | Shared formatting; command-specific content supplied by drivers |
| `bn_cli::process_log` | `src/process_log.rs`; `main.rs::{finish_process_log, mirror_frontend_diagnostics}` | Shared log lifecycle/redaction; build-specific default naming stays in compile driver |
| `bn_interpret_driver::run` | `main.rs::{run, run_loaded}` | Use shared frontend preparation and an already validated artifact |
| `bn_interpret_driver::eval` | `main.rs::{EvalMode, eval_source, eval, eval_top_level_start_span, eval_promotion_diagnostic, eval_line_partition, remap_eval_diagnostic, EvalMapping, eval_json_error}` | Snippet preparation, source-coordinate mapping and eval envelope; reuse common diagnostic serialization |
| `bn_interpret_driver::environment` | HostEnv construction in `main.rs::run_loaded`; `src/runtime.rs::HostEnvDefaults` | One environment/provider composition used by normal execution and DAP |
| `bn_interpret_driver::{hosts, libraries}` | `src/hosts.rs`, `src/hosts/{clock,console,exec,random}.rs`, `src/libraries.rs` | Move registries and lib-* feature ownership; retain existing provider crates |
| `bn_interpret_driver::jupyter_input` | `main.rs::JupyterInput` and its implementations | Preserve input protocol and JSON channel purity |
| `bn_compile_driver::build` | `main.rs::{build, build_inner, process_log_path}` | Shared frontend result, support check, policy carrier and build events; no re-lowering |
| `bn_compile_driver::options` | `main.rs::{Target, Optimization}` and build-only Options fields | Compiler-only option semantics |
| `bn_compile_driver::toolchain` | `src/cli_toolchain.rs` | clang/wasm-ld/archive discovery and configuration |
| `bn_compile_driver::artifact` | `main.rs::{emit_build_output, native_runtime_link_args}` | External tools, artifacts and platform link arguments |
| `bn_lsp` | `src/lsp.rs`, `src/lsp/{completion,tests}.rs` | Protocol adapter over the shared frontend/session |
| `bn_dap` | `src/dap.rs`, `src/dap/tests.rs` | Protocol adapter using the interpretation driver and existing debug hooks |

The wrapper `src/bnc.rs` contains another parser. Consolidate its common flags
and `normalize_log_level` with the common implementation after comparing
accepted syntax, aliases, defaults and diagnostics. Preserve or explicitly
decide differences such as quiet/log-dir behavior. Remove `locate_bn_binary`
and `build_bn_command` only when bnc calls the compilation library directly.

Root facades (`src/{lowering,llvm,ir,heap,runtime}.rs`) may temporarily preserve
imports during extraction. Their consumers, including integration tests, then
move to the owning crates. Moving only main.rs does not remove backend
dependencies from the root package's library.

## Shared preparation and policy

Source preparation belongs in `bn_frontend`, integrated with FrontendSession;
CLI rendering belongs in `bn_cli`. LSP must not consume a terminal-printing
function. DAP currently prepares in both `validate_launch` and `execute_program`.
Reuse preparation for the same snapshot/revision, or invalidate and prepare
again when inputs change; never reuse stale validated IR.

`bn_rt::Policy` remains the single policy type and environment parser.
Common flags describe restrictions; the interpretation driver builds HostEnv,
and the compilation driver supplies `bn_llvm::CompiledPolicy` as the emitted
policy carrier. The compiled runtime rechecks policy at the call boundary.
Do not add a second environment parser or a speculative bn_policy crate.
Archive discovery/linking for the produced program does not by itself require
the compiler executable to link bn_rt as a Rust dependency.

## Migration order and acceptance

1. Extract common CLI configuration, diagnostics, output and logging.
2. Consolidate frontend preparation and the shared check operation.
3. Extract the compilation driver; existing bn build consumes it.
4. Extract the interpretation driver and providers; existing bn run/eval consume it.
5. Extract LSP/DAP and migrate facade consumers, preserving existing commands.
6. Verify the extracted libraries through the existing executables.
7. Change bnc to call the compilation driver in-process.
8. Create bni as a small composition of the interpretation/protocol libraries.
9. Apply separately recorded compatibility, documentation, installer and release decisions.

Each extraction must satisfy GC-EXT and applicable GC-IR, GC-SUP, GC-FE,
GC-POL, GC-PAR and GC-DEP checks, preserving W1–W5. Both backend paths consume
language-validated IR; compilation checks target support before emission.
Tests must cover diagnostic equivalence, warning policy, module-path ordering,
JSON channels, policy denial and invalid policy, native/Wasm support rejection,
and protocol regressions. Retain existing test identities when moving tests.

Check normal dependency closures: bn_cli has no backend/providers; bni has no
bn_llvm; bnc has no bn_interp/tokio/hyper/rustls; bn_lsp has no backend. Execute
checks fail-closed, including failures of Cargo or the dependency scan itself.
Structural checks supplement behavioral evidence, not replace it. Follow the
bucket's sprint-end sequential test policy; build bn_rt before native linking
tests. No new executable is needed to prove the initial extraction works.
