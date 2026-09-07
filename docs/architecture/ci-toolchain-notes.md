# CI / native toolchain notes

Short invariants that blocked Basic Next `binaries.yml` publishes. Keep
`AGENTS.md` (local/gitignored) in sync when these change.

## Forbidden-deps needs ripgrep

`tests/check-forbidden-deps.sh` and `scripts/check-forbidden-deps.sh` require
`rg`. Without it, scans can look clean. The scripts fail closed if `rg` is
missing; the quality job installs ripgrep on Ubuntu runners.

## Native link needs `libbn_rt.a`

`bn build` links the `bn_rt` staticlib when IR references `@bn_rt_*`.
`configured_bn_rt_lib()` looks for `target/{debug,release}/libbn_rt.a` (or
`BN_RT_LIB`). `cargo test` alone may only leave a hashed archive under
`target/*/deps/`. Before native CLI / quality tests that compile HOST programs,
run `cargo build -p bn_rt`.

## LLVM PRINT sync is platform-specific

Native emit synchronizes PRINT with `flockfile` on libc stdout. The global
symbol is `__stdoutp` on Darwin/FreeBSD and `stdout` elsewhere
(`bn_llvm::helpers::stdout_file_symbol`). Do not hardcode Darwin-only
`__stdoutp` in emission.

## Related

- Workflow: `.github/workflows/binaries.yml`
- Local agent checklist: `AGENTS.md` (§ Changes and checks), gitignored on purpose
