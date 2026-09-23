# Prebuilt binaries

Binaries are not stored in Git. They are published on
[GitHub Releases](https://github.com/cquintella/BasicNext/releases).

Each release ships `bni` (interpreter) and `bnc` (compiler) per platform,
plus `libbn_rt` (native runtime for `bnc`), man pages `bni.1` / `bnc.1`, and
`SHA256SUMS`. Prefer `scripts/install.sh` / `scripts/install.ps1` when you want
a full prefix install.

| Platform | Interpreter | Compiler |
| --- | --- | --- |
| Linux x86_64 | [bni-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-x86_64) | [bnc-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-x86_64) |
| Linux aarch64 | [bni-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-aarch64) | [bnc-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-aarch64) |
| macOS Apple Silicon | [bni-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-aarch64) | [bnc-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-aarch64) |
| macOS Intel | [bni-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-x86_64) | [bnc-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-x86_64) |
| Windows x86_64 | [bni-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bni-windows-x86_64.exe) | [bnc-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-windows-x86_64.exe) |
