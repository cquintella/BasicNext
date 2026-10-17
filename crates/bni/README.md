# bni

Basic Next **interpreter** executable: `bni run|eval|check|lex|lsp|dap`.

This crate is a thin binary. Almost all command-line and execution logic lives
in libraries; `src/main.rs` only names the version string, wires the LSP and
DAP servers, and calls `bn_interpret_driver::cli::main`.

## Role

| Concern | Owner |
| --- | --- |
| Version line + protocol injection | this crate (`src/main.rs`) |
| Command dispatch, help, stack budget | `bn_interpret_driver` |
| Shared options, `check`, diagnostics, output | `bn_cli` |
| Language Server Protocol (stdio) | `bn_lsp` |
| Debug Adapter Protocol (stdio) | `bn_dap` |
| Tree-walking execution over validated IR | `bn_interp` (via the driver) |

Compilation is a separate executable: `bnc`. Architecture:
`docs/architecture/library-first-drivers.md`.

## Layout

```text
crates/bni/
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs          # entry point (≤ ~20 lines by design)
└── tests/
    ├── bni.rs
    ├── cli.rs           # CLI integration tests
    └── lsp_protocol.rs
```

## Entry

```text
src/main.rs
  → bn_interpret_driver::cli::main(VERSION, args, Protocols { lsp, dap })
```

`Protocols` is passed in because `bn_dap` depends on `bn_interpret_driver`;
the binary breaks that cycle by supplying `bn_lsp::run_stdio` and
`bn_dap::run_stdio` at the edge.

The driver runs the command on a dedicated thread with a large stack
(interpreter recursion must not depend on the OS default stack size).

## Commands

```text
bni <eval|check|lex|run|lsp|dap> [options] <file.bn> [-- program-args]
```

| Command | Role |
| --- | --- |
| `run` | Execute `FUNCTION Start` over typed, validated BN IR. Default if the first token is a path. |
| `eval` | Evaluate one source fragment (`SOURCE` or `--stdin`). |
| `check` | Lex, parse, semantic analysis, lower, validate. IDE Problems baseline. |
| `lex` | Print the token stream (`-o` writes it to a file). |
| `lsp` | Language Server Protocol over stdio. |
| `dap` | Debug Adapter Protocol over stdio. |

Interpreter-only options (via the driver’s `OptionExtension`):

| Flag | Notes |
| --- | --- |
| `--mode snippet\|program` | `eval` only |
| `--format text\|json` | `eval` only |
| `--jupyter-stdin` | `run` / `eval` |
| `--trace` | Report execution entry |
| `--no-filesystem` | Deny `HOST.FileSystem` (`run`) |
| `-o` / `--output` | Token stream of `lex` |

Common flags (`-v`, `--module-path`, `--sandbox`, warning policy, logging,
`--config`, …) come from `bn_cli`. See `bni --help` and `man bni`.

## Features

Default features enable the shipped `BN*` module adapters on the interpreter
path. Each maps to a feature of `bn_interpret_driver`:

| Feature | Module surface |
| --- | --- |
| `lib-math` | `BNMath` |
| `lib-json` | `BNJson` |
| `lib-log` | `BNLog` |
| `lib-data` | `BNData` |
| `lib-dispatch` | `BNDispatch` |
| `lib-web` | `BNWeb` |
| `lib-crypto` | `BNCrypto` |
| `lib-sqlite` | `BNSqlite` |

`bni --no-default-features` is language + HOST only (no LLVM; that stays in
`bnc`).

## Dependencies

- `bn_interpret_driver` — CLI and run/eval orchestration
- `bn_lsp` — `bni lsp`
- `bn_dap` — `bni dap`

Must **not** link the LLVM backend (`bn_llvm`).

## Build and run

From `bn/basicnext/`:

```bash
cargo build -p bni
cargo run -p bni -- run examples/hello.bn
cargo test -p bni
```

## Public API

None. This package is a binary only (`[[bin]] name = "bni"`). Reusable
surfaces are the library crates above, not `bni` itself.
