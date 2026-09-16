# Basic Next

![Basic Next Logo](docs/logo.png)

[![Rust CI](https://img.shields.io/badge/Rust_CI-passing-brightgreen)](#)
[![Version](https://img.shields.io/badge/version-v0.5.1-blue)](#)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](LICENSE.md)

An object-oriented, general-purpose programming language designed to reduce
cognitive load and turn ideas into clear, cross-platform software.

Basic Next combines BASIC-inspired readability, explicit types, and host
capabilities without prescribing a framework or architecture. It is designed
to make programming pleasurable: clear, predictable code should help
programmers sustain attention and enter a state of flow while reading and
writing software.

This repository starts with the specification: an implementation is introduced
only after the corresponding semantics have been defined and reviewed.

## Name

**Basic Next** (also written **BasicNext**) is not [NextBASIC](https://wiki.specnext.dev/NextBASIC),
the extended Sinclair BASIC interpreter that ships with the
[ZX Spectrum Next](https://www.specnext.com/). NextBASIC belongs to that retro
hardware and firmware ecosystem (NextZXOS / SpecNext). This project is a
separate, modern language with explicit types, a typed intermediate
representation, an interpreter (`bn run`), and an LLVM-backed compiler
(`bn build`). Please use the forms *Basic Next* / *BasicNext*; avoid the
compound spelling *NextBASIC* when referring to this repository.

## Design goals

- Readability before abbreviation.
- Low cognitive load, flow by clarity, and explicit contracts.
- KISS: complexity must solve a concrete problem.
- Every `LET` and `CONST` declaration states its type explicitly.
- Clean Code and Clean Architecture should be natural, never mandatory.
- Cross-platform software through `HOST` capabilities rather than vendor APIs.

Read [PHILOSOPHY.md](PHILOSOPHY.md) for the mission, vision, and complete set
of design principles.

## 🚀 Status: Version 0.5.1

Basic Next 0.5.1 builds on 0.5.0 with an evaluation subcommand (`bn eval`),
expressive structured diagnostics, an ordered module search path, and the
`HOST.Exec` capability with interpreter↔native parity. See
[What's New — 0.5.1](#whats-new--051) below.

The Basic Next reference implementation includes the Rust frontend, typed IR
interpreter, HOST capabilities, external BN modules, HTTP hardening, bounded
async runtime, debugger bridge, and notebook tooling. BNDispatch
native-provider conformance includes lifecycle, synchronization, isolation, and
networking corrections.

> **Note:** `bn build` is available for its supported typed-IR subset. The
> interpreter remains the reference implementation for language surfaces
> outside that subset.

## What's New — 0.5.1

- **`bn eval` subcommand.** Evaluate a source fragment from an argument or
  `--stdin` in one shot: `bn eval 'PRINT 1 + 1'`. A top-level `FUNCTION Start`
  auto-promotes to program semantics with one structured warning. `--format json`
  emits a single JSON v1 envelope (program stdout/stderr and structured
  `diagnostics[]`) with an otherwise-empty process stderr.
- **Expressive diagnostics.** Every user-visible toolchain diagnostic now carries
  structured facts (codes, typed arguments, primary/secondary spans, causes and
  help) from an external Fluent catalog under `share/bn/diagnostics/`, shared by
  the CLI, the `bn eval` JSON channel, and the LSP.
- **Ordered module search path.** Repeatable `--module-path <dir>` and a config
  `module-path = [...]` array; first hit wins, with the entry-file directory and
  the discovered stdlib `modules/bn` as defaults, applied identically to
  `check` / `run` / `build` / `eval`.
- **`HOST.Exec` capability.** `IMPORT HOST.Exec` exposes `Run(program, args)`:
  launch an OS executable without a shell, closed child stdin, concurrent bounded
  stdout/stderr capture, a wall-clock timeout, and a portable `Exec.Result OR
  Error` with signal-aware return codes — enforced by execution policy and
  available on both the interpreter and the native (LLVM) path.

## What's New — 0.5.0

Basic Next 0.5.0 makes automatic reference counting (ARC) the class lifetime
model. Strong assignments retain references, the last strong reference runs the
destructor, and `AS WEAK` references become `NULL` after destruction. `RELEASE`
can end a binding early; the `DELETE` keyword is not part of the 0.5.0 surface.

The external `BNString` module provides an object wrapper around primary
`STRING`, with Unicode case conversion, trimming, substring search, and a
separator tokenizer:

```basic
IMPORT BNString AS S
LET text AS S.String = NEW S.String("  Olá,BN  ")
LET clean AS S.String = text.Trim()
PRINT clean.LowCaps().ToString()
```

See [`docs/library/bnstring.md`](docs/library/bnstring.md) and
[`examples/bnstring_tour.bn`](examples/bnstring_tour.bn).

The 0.5.0 delivery train strengthens the native LLVM path while keeping the
typed BN IR validation boundary shared by `bn run` and `bn build`.

- The compiler capability catalog now covers the complete `examples/*.bn`
  roster. Supported fixtures are built and exercised as native artifacts; any
  remaining target limitation must use a stable `TARGET_UNSUPPORTED_*`
  diagnostic rather than masquerading as a language error.
- Native lowering and parity coverage expanded across calls and returns,
  counted loops, collections, nullable integers, object layout and inheritance,
  indexed assignment, input/EOF behavior, and selected HOST and standard-module
  surfaces.
- Compiled filesystem policy now carries sandbox roots into `bn_rt`, where it
  is re-checked at the file boundary. `BN_FS_POLICY=deny` and `read-only` can
  only narrow interpreter or compiled-artifact access.
- Rooted filesystem access is hardened against symlink replacement races on
  Unix through pinned directory descriptors with `openat`/`O_NOFOLLOW` and
  `unlinkat`; platforms without that primitive deny rooted access.
- The ABI index now checks that every LLVM-declared `bn_rt` function is both
  exported by the runtime archive and documented in the value/memory contract.

See the [capability catalog](tests/compiler-capabilities.json) for the machine
inventory of the supported compiler surface. The release tag remains subject to
the formal closeout commit and BDFL release acceptance.

## 🎯 Active implementation

The Basic Next reference implementation is a source-spanned lexer, handwritten recursive-descent/Pratt parser, syntax AST, semantic analyzer, typed BN IR, deterministic IR interpreter, and initial LLVM emitter. It provides:

- `bn check file.bn` — Accepts valid fixtures and reports precise, source-spanned diagnostics for errors.
- `bn run file.bn [-- args...]` — Validates and immediately executes the accepted interpreter surface.
- `bn build [--target native|wasm32] file.bn` — Emits LLVM IR, or an artifact with `-o`, for the supported compiler subset.

See the [0.5.0 contract](docs/0.5.0/language-0.5.0.md) for delivery status and accepted semantics.

To see under the hood, try:
- `bn check -v file.bn` (reports completed stages)
- `--emit ast`, `--emit typed-ast`, or `--emit ir` (emits frontend artifacts)

## 🛠️ Getting Started

`BN` is the official Basic Next tool, invoked as `bn`. It provides `bn check`,
`bn run`, and `bn build`; the commands share one diagnostic format, source
locations, and exit-code model.

### Quick Installation

**1. Install script (recommended)**
The install scripts build `bn` and `bnc` from source and place the binaries plus
the runtime files they need (standard-library `.bn` modules, diagnostics catalog,
and the Unix man page) in a single prefix. `bn` then discovers its modules and
catalog by walking upward from its own location — zero configuration afterwards.

Linux / macOS (installs to `/usr/local`, uses `sudo` only if that prefix is not
writable):
```shell
./scripts/install.sh
# user-local, no sudo:
PREFIX="$HOME/.local" ./scripts/install.sh
# custom prefix:
./scripts/install.sh --prefix /opt/basicnext
```

Windows (PowerShell; installs to `%LOCALAPPDATA%\Programs\BasicNext` and adds it
to your user `PATH`, no admin needed):
```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
# custom prefix:
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prefix C:\Tools\BasicNext
```

Installed layout (FHS):

| Path | Contents |
| --- | --- |
| `<prefix>/bin/bn`, `<prefix>/bin/bnc` | executables |
| `<prefix>/share/bn/modules/bn/*.bn` | standard-library modules |
| `<prefix>/share/bn/diagnostics/en-US/*.ftl` | diagnostics catalog (optional — an identical catalog is embedded) |
| `<prefix>/share/man/man1/bn.1` | Unix manual page (Linux/macOS) |

Each script verifies the install by running `bn --version` and resolving a
standard-library module from a clean directory. Pass `--no-build` (Bash) or
`-NoBuild` (PowerShell) to install already-built `target/release` binaries.

**2. For Users (Direct Download)**
Alternatively, download the pre-compiled binary for your operating system (Linux, macOS, Windows) directly from [GitHub Releases](https://github.com/cquintella/BasicNext/releases/latest).
The asset names and checksums are listed in [`binaries/README.md`](binaries/README.md).

**3. For Developers (Build from Source)**
If you prefer building from source and have Rust (1.97+) installed, you can install the CLI from this repository:
```shell
cargo install --path .
```
Note that `cargo install` places only the binary on your `PATH`; use the install
script above if you also want the stdlib modules and catalog on disk.

Usage, limits, and troubleshooting: [`docs/project/usage.md`](docs/project/usage.md).
Unix manual: [`bn(1)`](docs/man/bn.1) (`man docs/man/bn.1`).

The trivial case is zero-config: `bn run hello.bn` does not require a project
file or manifest. While developing from this repository, use:

```shell
cargo run -- run examples/hello.bn
cargo run -- run examples/language-tour.bn
cargo run -- check --emit ir examples/factorial.bn
cargo run -- --help
```

Release check from a clean tree:

```shell
cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings && git diff --check
```

Requires Rust 1.97. Current limitations include partial LLVM lowering;
`TIMEZONE` does not apply zone rules. Linked wasm32 modules run through
`node bin/bn-wasm`.

`config.toml` contains local tool configuration. Currently it selects the
`clang` command used by `bn build`; it does not alter language semantics.

## 📂 Repository layout

- `docs/book/en/` — English language tutorial ([toc](docs/book/en/toc.md)).
- `docs/language/0.2/` — accepted 0.2 language contract; `0.1/` is frozen.
- `todo/proposals/` — proposals not yet fully accepted.
- `done/proposals/` — accepted proposals kept for history.
- `docs/man/bn.1` — Unix man page for the `bn` tool.
- `docs/project/` — delivery planning, [usage](docs/project/usage.md), and the
  [experience contract](docs/project/experience-contract.md).
- `binaries/` — download index for prebuilt `bn` (binaries live on Releases).
- `examples/` — programs that guide the specification.
- [`examples/parallel-examples.md`](examples/parallel-examples.md) — bounded
  `BNDispatch` examples, including a parallel Leibniz-series pi calculation.
- Jupyter kernel — separate repository: [cquintella/basicnext-jupyter](https://github.com/cquintella/basicnext-jupyter).
- VS Code extension — separate repository: [cquintella/basicnext-vscode](https://github.com/cquintella/basicnext-vscode).
- `PHILOSOPHY.md` — design principles.
- `GOVERNANCE.md` — how decisions are made.
- `TRADEMARK.md` — use of the project name.

## ✨ Example (Hello World)

Basic Next is straightforward and designed for immediate readability ("Readable by design"):

```basic
FUNCTION Start() AS VOID
    PRINT "Bem Vindo ao Basic Next"
    LET counter AS INTEGER = 0

    WHILE counter < 10
        PRINT "Basic Next", counter
        counter += 1
    END WHILE
END FUNCTION
```

**Line by line explanation:**
- `FUNCTION Start() AS VOID` or `FUNCTION Start() AS INTEGER`: Every executable program begins with the `Start` function. LLVM emits an integer return as the process exit code; the interpreter supports both forms.
- `PRINT "Bem Vindo ao Basic Next"`: The built-in macro outputs text to the console.
- `LET counter AS INTEGER = 0`: Variable declarations are explicit (`LET`), always specify their type (`AS INTEGER`), and initialize their state.
- `WHILE counter < 10`: A standard loop with a clear pre-condition.
- `PRINT "Basic Next", counter`: `PRINT` can concatenate multiple expressions transparently.
- `counter += 1`: Safe, standard arithmetic mutation.
- `END WHILE` and `END FUNCTION`: Blocks are explicitly closed with named `END` statements, avoiding ambiguity and dangling braces.

*Check out `examples/language-tour.bn` for a complete demonstration of the language capabilities!*

---

“Basic Next é interessante quando tenta tornar explícitas as estruturas que outras linguagens escondem: tipos, escopo, memória, módulos, capacidades do host. Isso tem valor pedagógico e científico. Mas uma linguagem não se justifica por ser uma lista crescente de mecanismos; ela se justifica por revelar uma estrutura simples capaz de gerar muitas expressões úteis.”



## 🤝 Contributing
Read [CONTRIBUTING.md](CONTRIBUTING.md). Language evolution begins as proposals
in `todo/proposals/`; specification changes require examples.

## ❤️ Support
See [SPONSORSHIP.md](SPONSORSHIP.md) to support Basic Next maintenance without
interfering with its technical governance.

## 📄 License
[MPL 2.0](LICENSE.md).
