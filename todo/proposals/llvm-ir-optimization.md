# Proposal: LLVM optimization of build IR before native/wasm compile

**Status:** Implemented and verified 2026-09-04. This is the instruction for
`ongoing/bucket-0.4.3.md` activity 7.5 (`bn build --opt`). It is not
normative until `bn --help`, the 0.4 language/compiler text, and differential
fixtures match.

Verification 2026-09-04: `hello`, clock, Euclidean, `type_test`, and overflow
fixtures match `bn run` at `--opt none` and `--opt 2`; all five accepted option
values compile; invalid values are rejected by the CLI parser test.

**Not this proposal:** optimizing typed BN IR inside `bn run` (fold/DCE in
the interpreter). That is a separate pass. `bn run` must not invoke LLVM or
a JIT. This document is only: after textual LLVM IR is emitted, optimize it
with LLVM, then compile and link.

## Motivation

Today `bn build` writes a temporary `.ll` and invokes clang with no `-O`
(`src/main.rs` `emit_build_output`). Clang/wasm compile is `-O0` in
practice. The interpreter remains the semantic reference, but a compiled
binary should be allowed to run LLVM's optimizer on that IR so `bn build`
is a compiler, not a textual dump plus unoptimized codegen.

## Pipeline

```text
.bn → lexer → parser → semantics → typed BN IR
    → LLVM textual IR (existing lowering, overflow intrinsics, bn_rt calls)
    → LLVM optimize (this proposal)
    → clang / wasm32 compile + link (`bn_rt` unchanged)
```

Optimization happens on the generated LLVM IR, never by rewriting `.bn`
source or typed BN IR through a model. The lowering already uses
`llvm.*.with.overflow.*` and a `NUMERIC_OVERFLOW` trap; the optimizer must
see that IR, not a wrapping `add`.

## CLI

```text
bn build [--opt none|1|2|3|s] [--target native|wasm32] -o <out> <file.bn>
```

| Flag | Clang | Meaning |
| --- | --- | --- |
| `--opt none` | `-O0` | No LLVM opt; baseline for differential tests |
| `--opt 1` | `-O1` | |
| `--opt 2` | `-O2` | Default when `--opt` is omitted |
| `--opt 3` | `-O3` | |
| `--opt s` | `-Os` | |

Unknown `--opt` values are a tool error. `bn --help` documents the flag.
Wasm uses the same `--opt` on the wasm clang `-c` step.

Do not add a second `opt` binary requirement in profile 1: clang consuming
the `.ll` at the selected `-O` is the LLVM optimize-then-compile step.
A later optional `--emit llvm-opt` may dump the optimized IR; it is not
required to accept this proposal.

## Semantic contract

Compiled output at every `--opt` level must match `bn run` on the
differential fixtures:

- integer `+ - *` stay checked; overflow is `NUMERIC_OVERFLOW` (exit 1),
  never wrap;
- Euclidean `DIV` / `%`, shifts, and `**` keep interpreter diagnostics
  (`DIVISION_BY_ZERO`, `INVALID_SHIFT_COUNT`, `INVALID_EXPONENT`);
- no `-ffast-math`, no `nnan`/`ninf`/`nsz` that would change BN IEEE
  `NAN` / infinity / signed-zero rules;
- HOST / `bn_rt` calls are not deleted, reordered across observable I/O, or
  replaced with a different provider;
- `PRINT` / `INPUT` order is preserved.

If an LLVM pass would violate this contract, keep the lowering that blocks
it (overflow intrinsics, explicit traps, no `nsw` on wrapping-sensitive
ops). Do not silence a trap to win a benchmark.

## Implementation instruction

1. Parse `--opt` next to `--target` in `src/cli_frontend.rs`. Default `2`.
2. Pass the mapped `-O` to both native clang and wasm clang `-c`. Leave
   `wasm-ld` without an optimization-level flag unless a later measurement
   shows LTO is required; LTO is out of profile 1.
3. Keep linking `bn_rt` as today. Do not inline `bn_rt` by rewriting its
   source into the `.ll` in this proposal.
4. Help text, man page, and `bn --help` snapshots include `--opt`.
5. Tests:
   - `bn build --opt none` and `bn build --opt 2` of `examples/hello.bn`,
     `tests/grammar/valid/build-clock.bn`, Euclidean Start fixtures, and
     `examples/type_test.bn` once functions lower; stdout and exit codes
     match `bn run`;
   - overflow program still exits 1 under `--opt 2`;
   - invalid `--opt` is a tool error.
6. Close bucket row BN-043-018 when those tests are green.

## Deliberate exclusions

- No LLVM JIT in `bn run`.
- No typed-BN-IR optimizer in this document (fold/DCE for the interpreter
  stays a sibling activity).
- No new language syntax, HOST capability, or IR instruction kind.
- No `-ffast-math`, PGO, or LTO as a 0.4.3 requirement.
- No "optimize by emitting different BN".

## Acceptance

`bn build` without `--opt` is `-O2`. `--opt none` reproduces today's
unoptimized clang invocation. Optimized and unoptimized native binaries
agree with `bn run` on the fixtures above, including overflow and IEEE
specials.
