# Basic Next 0.2 compiler contract

## Strategy

The compiler reuses the existing lexer, parser, semantic model, and typed BN
IR. Sprint 9 lowers the supported IR subset with a dependency-free textual
LLVM emitter; `clang` performs assembly/linking for requested artifacts. The
interpreter remains the reference execution path and the Rust crate does not
link LLVM.

The approved baseline is LLVM 22.1.x (the version installed on the reference
development host); CI pins the matching major version rather than using a
floating system library.

## Targets

| Target | Artifact | Host capability policy |
| --- | --- | --- |
| Linux | ELF | `HOST.Console`, `HOST.Random`, `HOST.FileSystem` |
| macOS | Mach-O | `HOST.Console`, `HOST.Random`, `HOST.FileSystem` |
| Windows | PE/COFF | `HOST.Console`, `HOST.Random`, `HOST.FileSystem` |
| WebAssembly | linked `wasm32` module | `PRINT`, `INPUT`, `HOST.Args`, and seeded `HOST.Random`; filesystem and positioned console are unavailable |

Target selection is explicit and must never silently weaken a program's host
requirements. Unsupported capabilities are compile-time diagnostics.

The validation matrix is macOS arm64 (reference), Linux x86_64, Windows
x86_64, and `wasm32-unknown-unknown`. Native jobs verify the host capability
table; the WebAssembly job verifies capability refusal and module emission.

## Current CLI contract

`bn build [--target native|wasm32] <file.bn>` performs complete frontend
validation. Without `-o` it prints LLVM IR; with `-o` it invokes `clang` to emit
a native executable, or `clang` plus `wasm-ld` to emit a linked `wasm32`
module. Unsupported IR still exits with an
explicit `BUILD_LOWERING_UNAVAILABLE` diagnostic. Cross-target PE/COFF and
Mach-O object assembly is validated separately with `clang --target`; the
`native` CLI target links for the host platform.

## WebAssembly host ABI

The reference host is `bin/bn-wasm` and requires Node.js. Run a module with:

```text
node bin/bn-wasm program.wasm [args...]
```

The module exports `main`, `memory`, and `__heap_base`. It imports the portable
C-shaped functions `env.printf`, `env.putchar`, and, when needed,
`env.getchar` and `env.realloc`. The reference host implements the exact
formats emitted by the compiler (`%lld`, `%s`, and `%.17g`), copies
command-line arguments into linear memory, grows `INPUT` strings, forwards
standard input, and returns the BN exit code.

`bn build --target wasm32` uses a Clang that reports a `wasm32` target, then
links with `wasm-ld`. Discovery order: `BN_WASM_CLANG` / `BN_WASM_LD`,
`config.toml` `[toolchain] wasm-clang` / `wasm-ld`, `PATH`, Homebrew
`llvm` + `lld@20` (or `lld`), then Clang next to `wasm-ld`. Apple Clang on
macOS is not a WebAssembly compiler; Homebrew LLVM 22 is. No WASI sysroot
is required.
