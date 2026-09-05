# Using Basic Next 0.2

The `bn` tool checks, interprets, and compiles `.bn` programs through the same
source-spanned pipeline: lexer → parser/AST → semantic analysis → typed BN IR.

## Install

From this repository:

```shell
cargo install --path .
```

Or download a binary from [GitHub Releases](https://github.com/cquintella/BasicNext/releases/latest).
Building from source requires Rust 1.97. Native and WebAssembly compilation
also require Clang/LLVM 22; the WebAssembly target requires a Clang with a
`wasm32` backend and `wasm-ld` (not Apple Clang).

## Commands

```shell
bn check examples/hello.bn
bn lex examples/hello.bn
bn run examples/hello.bn -- extra-argument
bn build examples/hello.bn
bn build examples/hello.bn -o hello
bn build --target wasm32 examples/hello.bn -o hello.wasm
node bin/bn-wasm hello.wasm
```

| Command | Effect |
| --- | --- |
| `check` | Validate syntax and semantics. `--emit tokens|ast|typed-ast|ir` prints a frontend artifact. |
| `lex` | Print the token stream. |
| `run` | Execute `Start` through the typed-IR interpreter. |
| `build` | Emit LLVM IR, or create an artifact with `-o`, for the supported compiler subset. |

BN diagnostics exit `1`; invalid CLI use or unavailable build tooling exits
`2`. `-v` prints pipeline stages, and `-vv` also prints tokens.

`HOST.Args` belongs to the executable module and needs no import.
`HOST.Args[0]` is the absolute source path under `bn run`; later entries are
the values after `--`. Use `LEN(HOST.Args)` for the count.

`--no-filesystem` denies a `HOST.FileSystem` import before `Start`. The
Jupyter kernel uses this option. `--jupyter-stdin` is a private kernel/tool
protocol flag, not a normal user option.

The `wasm32` target currently supports `HOST.Args`, `HOST.Random`, `HOST.Clock`,
`HOST.Console`, string operations, and scalar `BNMath`. `HOST.FileSystem`,
`HOST.Net`, `BNLog`, and `BNWeb` are not in the WASI capability matrix yet;
`bn build --target wasm32` rejects those imports with
`BUILD_CAPABILITY_UNAVAILABLE` instead of emitting a non-functional socket
stub. WASI socket support is a follow-up provider contract.

## Modules and capabilities

User imports resolve beneath `modules/`. Standard modules such as `BNMath`
and `BNData` resolve beneath `modules/bn/` and require explicit `IMPORT`.
`HOST` is the only built-in interface object; accepted capabilities are
`HOST.Args`, `HOST.Clock`, `HOST.Console`, `HOST.Random`, and
`HOST.FileSystem`. Legacy argument and system namespaces are not part of 0.2.

WebAssembly artifacts run in the documented Node.js host:

```shell
node bin/bn-wasm program.wasm [args...]
```

The WebAssembly target supports stream I/O, arguments, and seeded random.
Filesystem and positioned-console operations are rejected for that target.

## Current limits

- LLVM lowering intentionally covers a typed scalar subset; unsupported IR
  produces `BUILD_LOWERING_UNAVAILABLE` instead of changing semantics.
- `TIMEZONE` stores an IANA identifier; UTC conversions do not apply zone
  rules.
- The VS Code adapter uses the native `bn dap` service for breakpoints, pause,
  continue, stack/scopes/variables, and step commands. Stepping is over IR
  instructions carrying source spans; it is not a REPL or arbitrary-expression
  evaluator.
- There is no package registry, network, GPU, DOM, or C FFI in 0.2.

Full command reference: [`bn(1)`](../man/bn.1). Tutorial:
[`docs/book/en/toc.md`](../book/en/toc.md).

## Troubleshooting

| Symptom | Resolution |
| --- | --- |
| `NAME_NOT_FOUND` on an imported member | Call it through the declared alias: `alias.member`. |
| `HOST_CAPABILITY_UNAVAILABLE` in Jupyter | The kernel intentionally denies filesystem and has no TTY. |
| `BUILD_TOOLCHAIN_UNAVAILABLE` | Install LLVM/Clang 22 and `wasm-ld`, or set `BN_WASM_CLANG` / `BN_WASM_LD` (Homebrew: `llvm` + `lld@20`). Apple Clang cannot emit `wasm32`. |
| `BUILD_LOWERING_UNAVAILABLE` | The valid program is outside the current compiler subset; use `bn run`. |
| `INPUT()` returns `EOF` | Standard input ended before a line was available. |
