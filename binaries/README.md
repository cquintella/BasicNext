# Prebuilt binaries

Compiled binaries are **not** stored in Git. GitHub Actions publishes them to
[Releases](https://github.com/cquintella/BasicNext/releases).

Since 0.6.0 a release ships three executables per platform: `bni` (the
interpreter, with `check`, `eval`, `lex`, `lsp` and `dap`), `bnc` (the
compiler) and `bn` (a compatibility dispatcher that forwards the 0.5 command
line to the other two; it retires in 0.7). The rolling **Latest** release
tracks `main`; a version tag such as `v0.6.0` attaches the same asset names.

## Latest download

| Platform | Interpreter | Compiler | Dispatcher |
| --- | --- | --- | --- |
| Linux x86_64 | [bni-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-x86_64) | [bnc-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-x86_64) | [bn-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-linux-x86_64) |
| Linux aarch64 | [bni-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-aarch64) | [bnc-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-aarch64) | [bn-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-linux-aarch64) |
| macOS Apple Silicon | [bni-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-aarch64) | [bnc-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-aarch64) | [bn-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-macos-aarch64) |
| macOS Intel | [bni-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-x86_64) | [bnc-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-x86_64) | [bn-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-macos-x86_64) |
| Windows x86_64 | [bni-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bni-windows-x86_64.exe) | [bnc-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-windows-x86_64.exe) | [bn-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bn-windows-x86_64.exe) |

Also attached to every release: `libbn_rt-<os>-<arch>.a` (`bn_rt-windows-x86_64.lib`)
— the native runtime `bnc` links —, the man pages `bni.1`, `bnc.1`, `bn.1`,
and `SHA256SUMS` covering every asset.

All releases: <https://github.com/cquintella/BasicNext/releases>

## After download

```bash
chmod +x bni-macos-aarch64 bnc-macos-aarch64   # Linux or macOS
./bni-macos-aarch64 --help
./bni-macos-aarch64 run hello.bn
./bnc-macos-aarch64 hello.bn -o hello          # needs clang and libbn_rt-macos-aarch64.a
```

On Windows, run `bni-windows-x86_64.exe --help`. macOS may require allowing
the binaries in System Settings the first time they are unsigned.

The dispatcher must be installed next to `bni` and `bnc` (or with both on
`PATH`) for `bn run` / `bn build` to keep working. `scripts/install.sh`
(`curl -fsSL … | bash`) does all of this and verifies the checksums.

Unix man pages from the same release:

```bash
sudo install -m 644 bni.1 bnc.1 bn.1 /usr/share/man/man1/
man bni; man bnc
```

The groff sources in the tree are [`docs/man/bni.1`](../docs/man/bni.1),
[`docs/man/bnc.1`](../docs/man/bnc.1) and [`docs/man/bn.1`](../docs/man/bn.1).

Rebuild locally with `cargo build --release --workspace --bins`; the outputs
are `target/release/{bni,bnc,bn}` and are gitignored.
