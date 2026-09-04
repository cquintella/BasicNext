# Proposals

A proposal states motivation, examples, and grammar or semantic impact. Nothing
in this directory is part of the language until it is incorporated into the
specification.

- [File I/O](../../done/proposals/file-io.md) — accepted into 0.2; see
  `docs/library/host.md`.
- [BNData (CSV and DataFrame)](../../done/proposals/bndata.md) — accepted
  into 0.2; normative text is `docs/library/bndata.md`.
- [Alternative types](../../done/proposals/alternative-types.md) — accepted
  for 0.1; 0.2 withdrew `Float.TryParse`.
- [LLVM IR optimization before compile](llvm-ir-optimization.md) — proposed 0.4.3 `bn build --opt` (clang `-O` on emitted LLVM IR). `bn run` stays off LLVM.
- [Expressive diagnostics](expressive-diagnostics.md) (target **0.4.5**) — Fluent shards by pipeline; lazy render; DiagId registry; per-code warning levels + overlay; no SQLite. Locked 2026-09-04.
- [HOST.Clock `Now` / `Timer`](../../done/proposals/host-clock-names.md) — accepted and implemented in 0.4.3; no alias.
- [BNText Markdown](bntext-markdown.md) — proposed portable Markdown text
  values for 0.3.
- [C Foreign Function Interface](c-ffi.md) — proposed `HOST.c` capability and
  a deliberately narrow C ABI profile.
- [Checked Numeric Semantics](numeric-semantics.md) — 0.1 rules in the
  interpreter; remaining work is negative fixtures (`DIVISION_BY_ZERO`,
  `INVALID_SHIFT_COUNT`, `INVALID_EXPONENT`, `INVALID_NUMERIC_CONVERSION`).
  Audit 2026-09-03.
- [Host capabilities](host-capabilities.md) — exploratory; not accepted.
- [Parallel computing](parallel-computing.md) — future `PARALLEL` syntax.
- [Native LSP & DAP](ide-tooling.md) — 0.3 surface mostly in tree; remaining: `--help`, VS Code `bn check` on save, find-references client. Audit 2026-09-03.
