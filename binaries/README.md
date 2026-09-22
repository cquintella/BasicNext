# Prebuilt binaries

Compiled binaries are **not** stored in Git. GitHub Actions publishes them to
[Releases](https://github.com/cquintella/BasicNext/releases).

Since 0.6.0 a release ships two executables per platform: `bni` (the
interpreter, with `check`, `eval`, `lex`, `lsp` and `dap`) and `bnc` (the
compiler). The 0.5 `bn` executable is gone (`bn run` → `bni run`, `bn build`
→ `bnc`). The rolling **Latest** release tracks `main`; a version tag such as
`v0.6.0` attaches the same asset names.

## Latest download

| Platform | Interpreter | Compiler |
| --- | --- | --- |
| Linux x86_64 | [bni-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-x86_64) | [bnc-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-x86_64) |
| Linux aarch64 | [bni-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-linux-aarch64) | [bnc-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-linux-aarch64) |
| macOS Apple Silicon | [bni-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-aarch64) | [bnc-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-aarch64) |
| macOS Intel | [bni-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bni-macos-x86_64) | [bnc-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-macos-x86_64) |
| Windows x86_64 | [bni-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bni-windows-x86_64.exe) | [bnc-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bnc-windows-x86_64.exe) |

Also attached to every release: `libbn_rt-<os>-<arch>.a` (`bn_rt-windows-x86_64.lib`)
— the native runtime `bnc` links —, the man pages `bni.1` and `bnc.1`, and
`SHA256SUMS` covering every asset.

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

`scripts/install.sh` (`curl -fsSL … | bash`) downloads all of this into one
prefix and verifies the checksums.

Unix man pages from the same release:

```bash
sudo install -m 644 bni.1 bnc.1 /usr/share/man/man1/
man bni; man bnc
```

The groff sources in the tree are [`docs/man/bni.1`](../docs/man/bni.1) and
[`docs/man/bnc.1`](../docs/man/bnc.1).

Rebuild locally with `cargo build --release --workspace --bins`; the outputs
are `target/release/{bni,bnc}` and are gitignored.
