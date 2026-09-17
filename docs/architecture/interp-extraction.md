# Interpreter extraction — measured blockers (input to the Fragilidade 2 bucket)

**Measured:** 2026-09-16 on `main` after bucket 0.5.1c sprints 1–3.1.
Every number below is reproducible with the command shown next to it. This
document does not decide anything; it tells the Fragilidade 2 bucket where the
interpreter (`src/runtime/**`, `src/runtime_impl.rs`) is still welded to the
`bn` god-crate.

## Layer gate status (Fragilidade 1)

| Check | State | Command |
| --- | --- | --- |
| Backend crates → `bn_frontend` in Cargo | none (`bn_llvm`, `bn_runtime`, `bn_value`, `bn_rt`, `bn_ir`) | `grep -E '^bn_' crates/*/Cargo.toml` |
| `scripts/forbidden-deps.allowlist` | **0 records**; checker fails on stale records | `bash scripts/check-forbidden-deps.sh` |
| Backend paths → any frontend module (`bn_frontend::`, `ast::`, `module_graph::`, `token::`, `frontend_session::`, `lowering::`, `keyword_registry::`, `semantic::`, `parser::`, `lexer::`, `ir::lower*`) | 0 | same checker; negatives in `tests/check-forbidden-deps.sh` (run by `tests/cli.rs::forbidden_dependency_gate_and_its_negatives_hold`) |
| `bn` public surface | `semantic` no longer re-exported; `crate::ir` = IR model only, `crate::lowering` = frontend | `cargo doc -p bn --no-deps`; `rg 'bn::semantic|ir::lower' src tests crates` → 0 |
| IR function identity | `Function::kind` / `owner` (structured, validated); name shapes informative; backends string-match only `@super:` and intrinsics — enforced by `tests/ir_names.rs` | `cargo test --test ir_names -p bn_ir` |

## What the interpreter still reaches inside the god-crate

Symbols the interpreter imports from `bn` host-implementation modules
(`rg -o --glob '*.rs' 'crate::<mod>::[A-Za-z_]+' src/runtime src/runtime_impl.rs | sort -u`):

| Module | Lines | Distinct symbols | Uses | Nature |
| --- | ---: | ---: | ---: | --- |
| `web` | 2 393 | 11 | 39 | BNWeb client/server, request/response, routing |
| `net` | 1 115 | 11 | 23 | HOST.Net addresses, sockets, resolver |
| `web_state` | 729 | 8 | 17 | sessions, cookie jars, egress policy state |
| `dispatch` | 1 096 | 7 | 13 | BNDispatch queues/tickets/sync primitives |
| `config` | 196 | 2 | 11 | tool configuration (clang command, limits) |
| `tls` | 172 | 2 | 5 | TLS config for HTTPS |
| `http` | 1 950 | 4 | 4 | HTTP/1.1–2 transport |
| `json` | 310 | 3 | 4 | BNJson provider |
| `log` | 3 | 2 | 2 | BNLog facade |
| `temporal` | — | 2 | 2 | temporal value helpers |
| `dataframe` | 13 | 0 | 0 | facade only (`bn_rt` provider) |
| `diagnostic` | — | — | 235 | `bn_diag` (already a crate — not a blocker) |

Interpreter size: `src/runtime/**` + `src/runtime_impl.rs` = 9 174 lines
(`cat src/runtime_impl.rs src/runtime/*.rs src/runtime/executor/*.rs | wc -l`).

## Extraction order that the numbers suggest

`bn_interp` cannot leave `bn` while it names `crate::web`, `crate::net`,
`crate::dispatch`, `crate::http`, `crate::tls`, `crate::json`,
`crate::web_state`, `crate::config`. Two viable shapes, to be decided in the
Fragilidade 2 bucket:

1. **Capabilities first.** Move each host implementation to its own crate
   (`bn_host_net`, `bn_host_web` (+ `web_state`, `http`, `tls`), `bn_host_dispatch`,
   `bn_host_json`) behind the existing `HostEnv`/provider traits; then
   `bn_interp` depends on those crates and `bn` becomes CLI + LSP + DAP.
   Order by fan-in: `net` (needed by `web`) → `dispatch` → `json` → `web`+`http`+`tls`+`web_state`.
2. **Interpreter first, providers injected.** `bn_interp` defines provider
   traits for the 10 modules above and `bn` supplies implementations at
   `HostEnv` construction. Smaller first step, but 40+ symbol seams to trait-ify
   and the capabilities stay in the god-crate.

The advisory's roadmap (step 3 then 4, "unify exec as the shared-core pilot")
matches shape 1 with `HOST.Exec` (already in `bn_rt`, E01–E14 on both backends)
as the template.

## Status after bucket 0.5.1d SPRINT 1 (2026-09-17)

Shape 2 was taken first, in place (D-F2-02): `src/runtime/provider.rs`
defines `Provider` (`call`, `allocate`, `release`, `close_all`,
`object_destroyed`, `as_any_mut`) and `CoreContext` (regions, object
allocation, `call_function`, `library_call`/`library_allocate`/`library_release`/
`library_mut`, output, module, host). Two `Providers` registries on `HostEnv`:
`libraries` (`BN*`, keyed by standard-module name, `src/libraries/`) and `hosts`
(HOST capabilities, keyed by the segment after `HOST.`, `src/hosts/`). Migrated:
`BNMath`, `BNJson`, `BNLog`, `BNData`, `BNDispatch`, `BNWeb`, `HOST.Net`.
Still in the core: `HOST.Exec`, `HOST.Clock`, `HOST.Args`, `HOST.FileSystem`
(`executor/part7.rs`, `part8.rs`) — SPRINT 3 moves them per capability, Exec
first. `rg 'crate::(web|web_state|net|dispatch|json|log|http|tls)::' src/runtime`
→ 0 outside `net_values.rs` (value projections) and one unit test.

## Not blockers (already clean)

- Value model: `bn_value::Value`; heap: `bn_runtime::Heap` (facade `src/heap.rs`).
- Types: `bn_types`; IR: `bn_ir`; diagnostics: `bn_diag`; spans: `bn_source`.
- Native runtime: `bn_rt` (no dependencies).
