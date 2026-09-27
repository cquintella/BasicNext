# `bn-fuzz` — frontend fuzz targets

Standalone [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) crate for
Basic Next. It feeds arbitrary UTF-8 bytes into the shared frontend to find
**panics and crashes** in the lexer and parser. It is not a product binary and
is not part of the root Cargo workspace.

## Layout

| Path | Role |
| --- | --- |
| `fuzz_targets/lex.rs` | `SourceFile` → `bn_frontend::lexer::lex` |
| `fuzz_targets/parse.rs` | `lex`, then `bn_frontend::parser::parse` on success |
| `corpus/` | Growing seed corpus (gitignored) |
| `artifacts/` | Crashing inputs when a run finds one (gitignored) |
| `target/` | Fuzzer build output (gitignored) |

Dependencies: `libfuzzer-sys`, path crates `bn_frontend` and `bn_source`.

## Why this directory is at the repo root (not under `tests/`)

`tests/` in this repository holds **Cargo integration tests** (`cargo test`,
`tests/*.rs`, parity scripts). Those expect the workspace packages and a normal
test harness.

`cargo-fuzz` expects a **separate package** with:

- `package.metadata.cargo-fuzz = true`
- its own `[workspace]` (so it is not a workspace member of the root crate)
- `fuzz_targets/` binaries built with libFuzzer

That layout is conventionally `fuzz/` at the repository root. Moving it under
`tests/` would fight the tool defaults, confuse “unit/integration test” with
“long-running fuzzer”, and still would not make `cargo test` run these targets.
Keep fuzzing here; keep `cargo test` under `tests/` and `crates/*/`.

## Prerequisites

- Nightly Rust (libFuzzer / `cargo-fuzz` requirement)
- `cargo install cargo-fuzz`

## Run

From the repository root:

```bash
cargo fuzz run lex
cargo fuzz run parse
```

Useful options (see `cargo fuzz run --help`):

```bash
cargo fuzz run parse -- -max_total_time=60
cargo fuzz run lex -- -runs=100000
```

Reproduce a crash file from `artifacts/`:

```bash
cargo fuzz run parse artifacts/parse/<crash-file>
```

## What “green” means

A successful fuzz session is **absence of sanitizer/libFuzzer crashes** for the
budget you ran — not a claim that the language accepts every input. Invalid
programs that return ordinary diagnostics are fine; panics, aborts, and memory
errors are not.

## History

Introduced on the closed 0.5.1 architecture line (see `done/bucket-0.5.1a.md`
and `AGENTS.md`). Corpus and artifacts stay local; only the crate and targets
are tracked.
