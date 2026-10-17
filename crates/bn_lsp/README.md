# bn_lsp

Language Server Protocol (LSP) adapter for Basic Next.

This crate is a library. The `bni lsp` command starts it over stdio by calling
`bn_lsp::run_stdio` from the thin `bni` binary. Analysis goes through the
shared frontend (`bn_frontend` / `FrontendSession`); diagnostics match the
IDE Problems baseline ≡ `bni check`.

## Role

| Concern | Owner |
| --- | --- |
| LSP transport (stdio, framing, initialize/shutdown) | this crate |
| Document snapshots and open/change/close | this crate + `FrontendSession` |
| Diagnostics (load → semantic → lower → validate) | this crate over `bn_frontend` |
| Completion, definition, references, hover, symbols, folding | this crate |
| Editor launch / UX | sibling repo `basicnext-vscode` (not this crate) |

## Boundaries

- Depends on `bn_frontend`, `bn_diag`, `bn_source`, `bn_types`, and the
  `lsp-server` / `lsp-types` crates.
- Must **not** depend on `bn_interp`, `bn_llvm`, `bn_cli`, or concrete HOST
  providers (frontend-only; same rule as architecture
  `library-first-drivers.md`).
- Does not own reusable CLI logic; `bni` injects `run_stdio` at the edge
  because `bn_dap` depends on the interpret driver.

## Layout

```text
crates/bn_lsp/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs              # run_stdio + request loop
│   └── lsp/
│       ├── completion.rs
│       ├── diagnostics.rs
│       ├── file_uri.rs
│       ├── folding.rs
│       ├── position.rs
│       └── tests.rs
└── tests/
    └── lsp_server.rs
```

## Capabilities

Advertised on `initialize` and handled in the request loop:

| LSP method | Support |
| --- | --- |
| `textDocument/didOpen` / `didChange` / `didClose` | Full document sync |
| `textDocument/publishDiagnostics` | After open/change; cleared on close |
| `textDocument/completion` | Keywords, builtins, modules, HOST / import members (trigger `.`) |
| `textDocument/definition` | Local and cross-document / sibling modules |
| `textDocument/references` | Find references |
| `textDocument/hover` | Hover |
| `textDocument/documentSymbol` | Outline |
| `textDocument/foldingRange` | Folding ranges |
| `shutdown` / `exit` | Lifecycle |

Message framing enforces a maximum payload size (`MAX_DOCUMENT_BYTES`, 8 MiB)
on the stdio transport.

## Exports

Public items are re-exported from the crate root (`src/lib.rs`).

| Item | Kind | Role |
| --- | --- | --- |
| `run_stdio` | `fn` | Run the LSP service over stdin/stdout. Entry used by `bni lsp`. |
| `diagnostics_for_documents` | `fn` | Multi-file diagnostic map for an open snapshot set (same pipeline as CLI check). |
| `compute_folding_ranges` | `fn` | Folding ranges for one `SourceFile`. |
| `span_to_lsp_range` | `fn` | BN span → LSP range. |
| `lsp_pos_to_scalar` | `fn` | LSP position → (line, scalar column). |
| `scalar_to_utf16_col` | `fn` | Scalar column → UTF-16 column (LSP). |
| `utf16_to_scalar_col` | `fn` | UTF-16 column → scalar column. |

Internal modules (`completion`, `file_uri`, request handlers) are not part of
the public API.

## How it is started

```text
bni lsp
  → bn_lsp::run_stdio()
```

From `crates/bni/src/main.rs`, `Protocols { lsp: bn_lsp::run_stdio, … }` is
passed into `bn_interpret_driver::cli::main`.

## Consumers

- `bni` — `bni lsp` command
- Integration tests in this crate and selected `bni` tests that call
  `diagnostics_for_documents` / `compute_folding_ranges` directly

## Build and test

From `bn/basicnext/`:

```bash
cargo test -p bn_lsp
cargo run -p bni -- lsp
```
