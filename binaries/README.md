# Prebuilt `bn` binaries
Compiled binaries are **not** stored in Git. GitHub Actions publishes them to
[Releases](https://github.com/cquintella/BasicNext/releases).


The rolling **Latest bn** release tracks `main`. A version tag such as
`v0.1.0` also attaches the same asset names.

## Latest download

| Platform | File |
| --- | --- |
| Linux x86_64 | [bn-linux-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-linux-x86_64) |
| Linux aarch64 | [bn-linux-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-linux-aarch64) |
| macOS Apple Silicon | [bn-macos-aarch64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-macos-aarch64) |
| macOS Intel | [bn-macos-x86_64](https://github.com/cquintella/BasicNext/releases/latest/download/bn-macos-x86_64) |
| Windows x86_64 | [bn-windows-x86_64.exe](https://github.com/cquintella/BasicNext/releases/latest/download/bn-windows-x86_64.exe) |
| Checksums | [SHA256SUMS](https://github.com/cquintella/BasicNext/releases/latest/download/SHA256SUMS) |
| Unix man page | [bn.1](https://github.com/cquintella/BasicNext/releases/latest/download/bn.1) |

All releases: <https://github.com/cquintella/BasicNext/releases>

## After download

```bash
chmod +x bn-macos-aarch64   # Linux or macOS
./bn-macos-aarch64 --help
./bn-macos-aarch64 run hello.bn
```

On Windows, run `bn-windows-x86_64.exe --help`. macOS may require allowing the
binary in System Settings the first time it is unsigned.

Unix man page from the same release:

```bash
sudo install -m 644 bn.1 /usr/share/man/man1/bn.1
man bn
```

The groff source in the tree is [`docs/man/bn.1`](../docs/man/bn.1).

Rebuild locally with `cargo build --release`; the output is `target/release/bn`
and is gitignored.
