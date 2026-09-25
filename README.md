# Basic Next

<p align="center">
  <img src="docs/logo.png" alt="Basic Next Logo" width="320" />
</p>

[![CI](https://github.com/cquintella/BasicNext/actions/workflows/binaries.yml/badge.svg)](https://github.com/cquintella/BasicNext/actions/workflows/binaries.yml)
[![Latest release](https://img.shields.io/github/v/release/cquintella/BasicNext)](https://github.com/cquintella/BasicNext/releases/latest)
[![License: MPL 2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](LICENSE.md)

---

An object-oriented, general-purpose programming language designed to reduce
cognitive load and turn ideas into clear, cross-platform software.

---

Basic Next combines BASIC-inspired readability, explicit types, and host
capabilities without prescribing a framework or architecture. It is designed
to make programming pleasurable: clear, predictable code should help
programmers sustain attention and enter a state of flow while reading and
writing software.

## Name

**Basic Next** (two words) is the language name. **BasicNext** is the repository
and package identifier.

This project is **not** [NextBASIC](https://wiki.specnext.dev/NextBASIC), the
extended Sinclair BASIC that ships with the
[ZX Spectrum Next](https://www.specnext.com/).

Basic Next 0.6 ships two executables over one shared frontend — `bni`, the
interpreter, and `bnc`, the LLVM-backed compiler. The 0.5 `bn` command is
gone: `bn run …` is `bni run …` and `bn build …` is `bnc …`.

- `bni run` — validate, lower to BN IR, and interpret
- `bnc` — compile the supported IR subset to a native or Wasm artifact
- `bni eval` — evaluate a source fragment (argument or `--stdin`) without a temp file
- `bni check` / `bni lex` / `bni lsp` / `bni dap` — check, lex, language server, debugger

## Design goals

- Readability before abbreviation.
- Low cognitive load, flow by clarity, and explicit contracts.
- KISS principle: complexity must solve a concrete problem.
- Every `LET` and `CONST` declaration states its type explicitly.
- Clean Code and Clean Architecture should be natural, never mandatory.
- Cross-platform software through `HOST` capabilities rather than vendor APIs.

Read [PHILOSOPHY.md](PHILOSOPHY.md) for the mission, vision, and complete set
of design principles.

## 🚀 Status: 0.6.1 released; 0.6.2 in development

**Latest GitHub release:** [v0.6.1](https://github.com/cquintella/BasicNext/releases/tag/v0.6.1).
The 0.6 line ships `bni` and `bnc` (the 0.5 `bn` executable is gone). Normative
language contract: [`language/0.6/`](language/0.6/0.6.md). Release notes:
[`docs/releases/`](docs/releases/README.md).

The tutorial source under `docs/book/en/` and the normative language contract
both track the 0.6 command and language surface.


## 🛠️ Getting Started

The toolchain is two executables: `bni` (`check`, `run`, `eval`, `lex`, `lsp`,
`dap`) and `bnc` (`bnc [compile-options] <entry.bn>`). They share one
frontend, one diagnostic format, source locations, and exit-code model.

### Quick Installation

**Linux / macOS — verified release installer (from v0.6.1)**

The commands below become available when `v0.6.1` is published. Until then,
use the source-checkout instructions below; the `v0.6.0` release did not ship
the versioned installer and source payload.

```shell
version=v0.6.1
base="https://github.com/cquintella/BasicNext/releases/download/$version"
curl --proto '=https' --tlsv1.2 -fSLO "$base/install.sh"
curl --proto '=https' --tlsv1.2 -fSLO "$base/SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
    grep ' install.sh$' SHA256SUMS | sha256sum -c -
else
    grep ' install.sh$' SHA256SUMS | shasum -a 256 -c -
fi
bash install.sh
```

Downloading, verifying, and executing are separate steps so the script is never
executed directly from a mutable branch. The script asks where to install:

1. `$HOME/basicnext`
2. `/opt/basicnext`
3. `/usr/local`
4. Other path…

It then installs the release's `bni`/`bnc`, standard-library modules,
diagnostics catalog, and man pages under that prefix. The binaries and source
payload are checked against the release's `SHA256SUMS`. If the release has no
binary for your platform, it builds from that verified source payload with
`cargo`. Paths under your home need no `sudo`; `/usr/local` and
`/opt/basicnext` use `sudo` when the directory is not writable.

From a clone, the same menu applies (`./scripts/install.sh` builds from source). Skip
the menu with `--prefix DIR` or `PREFIX=DIR`. Pin a release that publishes the
verified installer payload with `BN_VERSION=v0.6.1`.

A successful install also writes `$HOME/.basicnext/install.log` and
`$HOME/.basicnext/uninstall.sh` (override with `BN_STATE_DIR`).

**Windows (PowerShell, from v0.6.2)** — default prefix
`%LOCALAPPDATA%\Programs\BasicNext` (no admin):

```powershell
$Release = Invoke-RestMethod -UseBasicParsing `
    -Uri 'https://api.github.com/repos/cquintella/BasicNext/releases/latest'
$Version = $Release.tag_name
$Base = "https://github.com/cquintella/BasicNext/releases/download/$Version"
$Work = Join-Path ([System.IO.Path]::GetTempPath()) `
    ("basicnext-bootstrap-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$PreviousVersion = $env:BN_VERSION
try {
    $Installer = Join-Path $Work 'install.ps1'
    $Checksums = Join-Path $Work 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/install.ps1" -OutFile $Installer
    Invoke-WebRequest -UseBasicParsing -Uri "$Base/SHA256SUMS" -OutFile $Checksums
    $Expected = ((Select-String -Path $Checksums -Pattern ' install.ps1$').Line -split '\s+')[0]
    $Actual = (Get-FileHash $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw 'install.ps1 checksum mismatch' }
    $env:BN_VERSION = $Version
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Installer
    if ($LASTEXITCODE -ne 0) { throw "install.ps1 exited with $LASTEXITCODE" }
} finally {
    $env:BN_VERSION = $PreviousVersion
    Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
}
```

The bootstrap and installer use PowerShell-native networking, hashing, and ZIP
extraction; `curl.exe` and `tar.exe` are not required.

Installed layout (FHS):

| Path | Contents |
| --- | --- |
| `<prefix>/bin/bni`, `<prefix>/bin/bnc` | executables (interpreter, compiler) |
| `<prefix>/share/bn/modules/bn/*.bn` | standard-library modules |
| `<prefix>/share/bn/diagnostics/en-US/*.ftl` | diagnostics catalog (optional — an identical catalog is embedded) |
| `<prefix>/share/man/man1/{bni,bnc}.1` | Unix manual pages (Linux/macOS) |

Each script verifies the install by running `bni --version` and resolving a
standard-library module from a clean directory. Pass `--no-build` (Bash) or
`-NoBuild` (PowerShell) to install already-built `target/release` binaries.

**Direct download**
Alternatively, download the pre-compiled binary for your operating system (Linux, macOS, Windows) directly from [GitHub Releases](https://github.com/cquintella/BasicNext/releases/latest).
The asset names and checksums are listed in [`binaries/README.md`](binaries/README.md).

**Build from source**
If you prefer building from source and have Rust **1.98** installed (see `rust-toolchain.toml`), you can install the CLI from this repository:
```shell
cargo install --path .
```
Note that `cargo install` places only the binary on your `PATH`; use the install
script above if you also want the stdlib modules and catalog on disk.

Usage, limits, and troubleshooting: [`docs/project/usage.md`](docs/project/usage.md).
Unix manuals: [`bni(1)`](docs/man/bni.1) and [`bnc(1)`](docs/man/bnc.1) (`man docs/man/bni.1`).

The trivial case is zero-config: `bni run hello.bn` does not require a project
file or manifest. While developing from this repository, use:

```shell
cargo run -p bni -- run examples/hello.bn
cargo run -p bni -- run examples/language-tour.bn
cargo run -p bni -- run examples/filesystem_tour.bn
cargo run -p bni -- run examples/bnlog_tour.bn
cargo run -p bni -- eval 'PRINT 1 + 1'
cargo run -p bni -- check --emit ir examples/factorial.bn
cargo run -p bnc -- examples/hello.bn -o hello
cargo run -p bni -- --help
```

Release check from a clean tree:

```shell
cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings && git diff --check
```

Requires Rust **1.98** (`rust-toolchain.toml`). Current limitations include partial LLVM lowering;
`TIMEZONE` does not apply zone rules. Linked wasm32 modules run through
`node bin/bn-wasm`.

`config.toml` contains local tool configuration. Currently it selects the
`clang` command used by `bnc`; it does not alter language semantics.


## 📂 Repository layout

- `docs/book/en/` — English language tutorial ([toc](docs/book/en/toc.md)).
- `language/0.6/` — **normative** 0.6.x language contract ([`0.6.md`](language/0.6/0.6.md), EBNF, keywords). Older `0.2/`–`0.5/` trees remain historical.
- `todo/proposals/` — proposals not yet fully accepted.
- `done/` — closed buckets and accepted proposal history (often local / gitignored).
- `docs/man/bni.1`, `docs/man/bnc.1` — Unix man pages.
- `docs/project/` — delivery planning, [usage](docs/project/usage.md), and the
  [experience contract](docs/project/experience-contract.md).
- `binaries/` — download index for prebuilt `bni`/`bnc` (binaries live on Releases).
- `examples/` — programs that guide the specification (see below).
- [`examples/parallel-examples.md`](examples/parallel-examples.md) — bounded
  `BNDispatch` examples, including a parallel Leibniz-series pi calculation.
- Editor support: [basicnext-vscode](https://github.com/cquintella/basicnext-vscode)
  (standalone VS Code extension; Jupyter kernel is also a separate repository).
- Jupyter kernel — separate repository: [cquintella/basicnext-jupyter](https://github.com/cquintella/basicnext-jupyter).
- `PHILOSOPHY.md` — design principles.
- [`docs/governance.md`](docs/governance.md) — how decisions are made.
- [`docs/trademark.md`](docs/trademark.md) — use of the project name.

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

### Examples worth running

| Program | Focus |
| --- | --- |
| [`examples/hello.bn`](examples/hello.bn) | Minimal `Start` |
| [`examples/language-tour.bn`](examples/language-tour.bn) | Broad language surface |
| [`examples/filesystem_tour.bn`](examples/filesystem_tour.bn) | `HOST.FileSystem` write / read / close |
| [`examples/bnlog_tour.bn`](examples/bnlog_tour.bn) | `BNLog` console + file transports |
| [`examples/bnstring_tour.bn`](examples/bnstring_tour.bn) | `BNString` |
| [`examples/bndata_tour.bn`](examples/bndata_tour.bn) | `BNData` + CSV via FileSystem |
| [`examples/exec-demo.bn`](examples/exec-demo.bn) | `HOST.Exec` (when present in tree) |
| [`examples/socket.bn`](examples/socket.bn) | `HOST.Net` TCP/UDP |

Prefer `cargo run -p bni -- run …` from a checkout so you use this tree’s
toolchain (a globally installed `bni` may lag behind `main`).

---

“Basic Next é interessante quando tenta tornar explícitas as estruturas que outras linguagens escondem: tipos, escopo, memória, módulos, capacidades do host. Isso tem valor pedagógico e científico. Mas uma linguagem não se justifica por ser uma lista crescente de mecanismos; ela se justifica por revelar uma estrutura simples capaz de gerar muitas expressões úteis.”



## 🤝 Contributing
Read [docs/contributing.md](docs/contributing.md). Language evolution begins as proposals
in `todo/proposals/`; specification changes require examples.

Security vulnerabilities should be reported privately according to
[SECURITY.md](SECURITY.md), not through a public issue.

## ❤️ Support
See [docs/sponsorship.md](docs/sponsorship.md) to support Basic Next maintenance without
interfering with its technical governance.

## 📄 License
[MPL 2.0](LICENSE.md).
