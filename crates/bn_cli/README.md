# bn_cli

Shared command-line services for the Basic Next executables `bni` and `bnc`.

This crate is a library, not a binary. The executables stay thin: `bni` calls
`bn_interpret_driver`, `bnc` calls `bn_compile_driver`, and both drivers use
`bn_cli` for options, frontend preparation, `check`, diagnostics, output, and
the companion process log.

## Role

| Concern | Where it lives |
| --- | --- |
| Common flags, config, warning policy | `options` (config discovery is private) |
| Read / lex / prepare entry source | `frontend` |
| `bni check` (IDE Problems baseline) | `check` |
| Text and JSON diagnostic presentation | `diagnostics` |
| Exit codes, color, emit helpers | `output` |
| Shared `--help` fragments | `help` |
| Companion process log | `process_log` |

## Boundaries

- Depends on `bn_frontend`, `bn_diag`, `bn_source`, and `bn_catalog`.
- Must **not** depend on `bn_interp`, `bn_llvm`, or concrete HOST providers.
- Driver-specific flags (`--target`, `--opt`, run/eval options, …) are parsed
  through `OptionExtension`; this crate never learns them.

Architecture: `docs/architecture/library-first-drivers.md`.

## Public modules

```text
bn_cli
├── check
├── diagnostics
├── frontend
├── help
├── options
├── output
└── process_log
```

`config` exists in the crate but is **not** public; drivers reach configuration
only through `options::parse_options`.

## Exports

### `bn_cli::check`

| Item | Kind | Role |
| --- | --- | --- |
| `check` | `fn` | Prepare the program and report success (or fail with diagnostics). |
| `frontend_artifact` | `fn` | Format a `--emit` artifact (tokens, AST, typed AST, IR). |

### `bn_cli::diagnostics`

| Item | Kind | Role |
| --- | --- | --- |
| `render_diagnostic` | `fn` | Catalog + warning-policy text rendering (incl. eval remapping). |
| `diagnostic_json` | `fn` | One diagnostic object of the JSON v1 envelope. |
| `emit_frontend_error` | `fn` | Print one frontend error; exit as language error. |
| `emit_frontend_warnings` | `fn` | Apply warning policy; `true` if any warning is fatal. |
| `tool_diagnostic` | `fn` | Toolchain diagnostic with no source position. |

### `bn_cli::frontend`

| Item | Kind | Role |
| --- | --- | --- |
| `Frontend` | `type` | Alias for `bn_frontend::prepare::Prepared`. |
| `read_source` | `fn` | Read `options.path` and lex it. |
| `load_frontend` | `fn` | Prepare from disk via `bn_frontend::prepare`. |
| `load_frontend_with_overlays` | `fn` | Prepare with in-memory path overlays. |

### `bn_cli::help`

| Item | Kind | Role |
| --- | --- | --- |
| `COMMON_OPTIONS` | `const &str` | Help lines for flags both executables honor. |
| `BOOK_URL` | `const &str` | Tutorial book URL referenced from `--help`. |
| `usage` | `fn` | Print one-line usage; tool-error exit. |
| `option_error` | `fn` | Reject a bad option and print full help. |
| `tool_source` | `fn` | Driver convenience: `read_source` for a command. |

### `bn_cli::options`

| Item | Kind | Role |
| --- | --- | --- |
| `Emit` | `enum` | `Tokens`, `Ast`, `TypedAst`, `Ir`, `Llvm`. |
| `OutputFormat` | `enum` | `Text`, `Json`. |
| `Color` | `enum` | `Auto`, `Always`, `Never`. |
| `EvalMapping` | `struct` | Source-coordinate remapping for wrapped `eval` text. |
| `Options` | `struct` | Common parsed options (path, verbosity, sandbox, logs, …). |
| `OptionExtension` | `trait` | Driver-supplied parser for unknown flags. |
| `parse_options` | `fn` | Parse common options after the command word. |

`()` implements `OptionExtension` (no extension flags).

### `bn_cli::output`

| Item | Kind | Role |
| --- | --- | --- |
| `language_error` | `fn` | Exit code 1 (invalid program / language terms). |
| `tool_error` | `fn` | Exit code 2 (toolchain / CLI / I/O failure). |
| `color_enabled` | `fn` | Whether ANSI color should be used. |
| `colorize` | `fn` | Optionally wrap status text in green ANSI. |
| `log` | `fn` | Pipeline-stage line on stderr when verbosity allows. |
| `module_index` | `fn` | Convert a module id to a slice index. |
| `tokens_text` | `fn` | Format lexed tokens for `-vv` / emit. |
| `emit_output` | `fn` | Write an artifact to a path or stdout. |

### `bn_cli::process_log`

| Item | Kind | Role |
| --- | --- | --- |
| `LogLevel` | `enum` | `Error`, `Warn`, `Info`, `Debug` (+ `parse`). |
| `ProcessLog` | `struct` | Buffered events; `new`, `event`, `finish`, … |
| `LogTarget` | `enum` | `Explicit` path or `Companion` beside the product. |
| `companion_log` | `fn` | Path of `<product>.bnbuild.log` when applicable. |

`ProcessLog` methods: `new`, `record_warning`, `warning_count`, `event`,
`write_to`, `finish`, `mirror_frontend_diagnostics`.

`LogTarget` methods: `path`.

## Consumers

- `bn_interpret_driver` (`bni`: run, eval, check, lex, …)
- `bn_compile_driver` (`bnc`: compile, emit, link)

## Entry

There is no `main`. The crate root is `src/lib.rs`.
