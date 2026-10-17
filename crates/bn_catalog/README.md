# bn_catalog

Text catalog for Basic Next: every user-visible string and every contract
name that code looks up by identifier lives here, not as a string literal in
Rust (`AGENTS.md`, “No hard-coded messages”; bucket 0.6.5 section H).

evita que tenha hardcodded no codigo do bn

The source of truth is `share/bn/catalog/<domain>.txt`. Those files are
**embedded at build time** (`include_str!`), so `bni`, `bnc`, and compiled
programs do not need the catalog on disk at run time.

## Role

| Concern | Where it lives |
| --- | --- |
| Domain text files (`key = value`) | `share/bn/catalog/*.txt` |
| Embed, parse, lookup, fill | this crate (`src/lib.rs`) |
| Key ↔ use gate (every named key exists; every file key is used) | `tests/catalog_keys.rs` |

Diagnostics (Fluent codes and messages) are a separate catalog under
`share/bn/diagnostics/`, owned by `bn_diag`. This crate is for CLI texts,
HOST/module contract names, runtime messages, and similar domain strings.

## File format

One entry per line:

```text
# comment
key.name = value text with optional {placeholders}
```

- Empty lines and lines starting with `#` are ignored.
- Separator is exactly ` = ` (space-equals-space).
- `{name}` placeholders are filled by `fill`; unknown braces stay as written.

## Domains (embedded files)

| Domain | File |
| --- | --- |
| `bnlog` | `share/bn/catalog/bnlog.txt` |
| `cli` | `share/bn/catalog/cli.txt` |
| `config` | `share/bn/catalog/config.txt` |
| `crypto` | `share/bn/catalog/crypto.txt` |
| `data` | `share/bn/catalog/data.txt` |
| `env` | `share/bn/catalog/env.txt` |
| `exec` | `share/bn/catalog/exec.txt` |
| `file` | `share/bn/catalog/file.txt` |
| `frontend` | `share/bn/catalog/frontend.txt` |
| `json` | `share/bn/catalog/json.txt` |
| `llvm` | `share/bn/catalog/llvm.txt` |
| `net` | `share/bn/catalog/net.txt` |
| `web` | `share/bn/catalog/web.txt` |

Adding a domain means a new `.txt` under `share/bn/catalog/` **and** a new
entry in `FILES` in this crate.

## Exports

All public items live at the crate root (`src/lib.rs`).

| Item | Kind | Role |
| --- | --- | --- |
| `FILES` | `const &[(&str, &str)]` | `(domain, contents)` of every embedded catalog file. |
| `parse` | `fn` | Parse one file body into `(key, value)` pairs (file order). |
| `text` | `fn` | Look up `domain` / `key`; returns `&'static str`. |
| `words` | `fn` | Whitespace-split the text of `domain` / `key`. |
| `fill` | `fn` | Look up `domain` / `key` and replace `{name}` from a value table. |

### Lookup contract

```rust
bn_catalog::text("cli", "option.emit.expects");
bn_catalog::fill("exec", "run.message", &[("program", &name)]);
bn_catalog::words("bnlog", "some.word.list");
```

`text` (and therefore `fill` / `words`) **panics** if the key is missing.
That is treated as a caller defect; `tests/catalog_keys.rs` must catch it
before release.

## Consumers

Used across the toolchain wherever a user-readable or contract string must
not be hard-coded, including `bn_cli`, the interpret/compile drivers,
`bn_core_*` / `bn_lib_*`, `bn_rt`, `bn_llvm`, and HOST cores.

Some domains wrap this crate behind a thin local API (for example
`bn_core_log::catalog`) so call sites stay domain-scoped; the data still
comes from `FILES` here.

## Entry

There is no `main`. The crate root is `src/lib.rs`.
