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
- [C Foreign Function Interface](c-ffi.md) — proposed `HOST.c` capability and a deliberately narrow C ABI profile. Distinct from architecture [native-stdlib-binding](../../docs/architecture/native-stdlib-binding.md) (stdlib → shared `bn_rt`).
- [Checked Numeric Semantics](numeric-semantics.md) — 0.1 rules in the
  interpreter; remaining work is negative fixtures (`DIVISION_BY_ZERO`,
  `INVALID_SHIFT_COUNT`, `INVALID_EXPONENT`, `INVALID_NUMERIC_CONVERSION`).
  Audit 2026-09-03.
- [Host capabilities](host-capabilities.md) — exploratory; not accepted.
- [HOST.Ui + BNUI (v0)](host-ui-bnui-v0.md) — Flow-only UI capability/module; no EVENT keyword; absolute/scroll deferred. Proposed 2026-09-07.
- [HOST.SQLite (v0)](host-sqlite-v0.md) - Exec -> VOID OR Error; Query -> Data.DataFrame OR Error; BNData integration. Proposed 2026-09-07.
- Parallel computing — future `PARALLEL` syntax; proposal not yet materialized as a document.
- [Native LSP & DAP](../../docs/architecture/README.md) — 0.3 surface mostly in tree; remaining: `--help`, VS Code `bn check` on save, find-references client. Audit 2026-09-03.
- [BNString extras](string-extras.md) — stdlib Split/Join/Contains/IndexOf/Trim (**0.4.7** G7.5); interpolation `$"..."` deferred (language DNA). Draft expanded 2026-09-07.
- [Typed Dispatch results](dispatch-typed-return.md) — language/`AWAIT` (or `Ticket.Result`) to surface worker returns; `bn_rt` already has result pointer. Proposed 2026-09-07.
- [Bucket 0.5.0 corrective](bucket-0.5.0-corrective.md) — ARC memory lock (strong/weak) + typed dispatch AWAIT; plan only. Proposed 2026-09-12.
- [bn -e eval mode (0.5.1)](bn-eval-mode-0.5.1.md) — global `-e`/`--expr` eval/oneshot (+ optional session) for `bnr` / hosts using installed `/usr/local/bin/bn`; not a `bn eval` subcommand; not in 0.5.0 claim. Proposed 2026-09-12; lock `-e` 2026-09-13.
- [Early `Error` propagation](error-propagation.md) — proposed postfix `?` (or CHECK/PROPAGATE); language DNA; desugars to IF/RETURN; does **not** change toolchain diagnostics.
- [BNData Expansion (Series & Analytics)](bndata-expansion.md) — proposed 1D dynamic series, NA semantics, vectorized operations, and DataFrame enhancements.
