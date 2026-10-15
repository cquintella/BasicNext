# Bucket 0.6.9b — native `STRING` as a two-word value (stage 2 of 2)

**Status:** Deferred (`todo/`), created 2026-10-15 (Carlos: string work in
two stages). Lane `0.6.9b` because `bucket-0.6.9.md` is `bnr`.
**Depends on:** Sprint 7 (G.S2) of `ongoing/bucket-0.6.5.md` closed, i.e.
stage 1: every native `STRING` points to a payload after a `StringHeader`
(G7-D1, G7-D2), ownership comes from the IR (G7-D5), and K1 and K22 are closed.
K44 (P.6, `STATIC` shared by `ASYNC` tasks under `bnc`), which D4 needs,
closed on 2026-10-15.
**Kind:** toolchain only. No grammar, keyword or semantic change; the
observable behaviour of every program stays identical to `bni`.

Claim: in `bnc` programs, a `STRING` value is `{ ptr, len|flags }` in
registers, locals, fields and vector elements. Small strings may live inside
the value with no allocation, and substrings may avoid a copy, *only where
the measurement of activity M shows a gain*. The heap layout of stage 1 is
unchanged.

## Why a second stage

Stage 1 fixes correctness with a one-word value. Stage 2 is the Swift design
(`_StringGuts`): the value carries the length and a few flags, the heap
object carries the count. What it adds over stage 1, and what it costs:

| Gain | Weight for BN |
| --- | --- |
| Small-string storage (SSO): up to 15 bytes inside the 16-byte value, no `malloc` | Possibly large: BASIC-style programs use many short strings. Every operation branches on small/large |
| Substring without copy | Only if the language has slicing operations that are hot. Today `STRING[i]` returns one scalar and `BNString` has `Contains`/`Trim`; a slice must keep its owner alive (third word or separate representation) |
| Static and borrowed buffers marked in `flags` instead of an immortal count | Small: stage 1 already gives literals a static header |
| `len` without a load of `ptr - 8` | Small: the header shares a cache line with the payload |

| Cost | Detail |
| --- | --- |
| Emitter rewrite | 385 string references in 49 files of `bn_llvm` (25,516 lines, 2026-10-15); `tests/source_size.rs` LLVM budget |
| Platform ABI | A 16-byte struct is returned in two registers on SysV and AArch64, through a hidden pointer on Windows x64, and needs multi-value or an out-pointer on wasm32. Hence D5 |
| Tearing | A 16-byte store is not atomic. Safe only while no `STRING` slot is written by one thread and read by another (D4) |
| C boundary | A small or sliced string has no trailing NUL; passing it to C needs a copy (`todo/proposals/c-export.md` fixes `STRING` parameters as `const char *` NUL-terminated, borrowed) |

## Facts at creation (verify again at start)

- Stage 1 value: `layout.rs` `llvm_type` maps `Type::String` to `Ptr`.
- Heap header (G7-D2): `[ ref_count: AtomicU64 ][ len: u64 ][ payload ][ 0 ]`,
  pointer = `base + 16`, `ref_count == u64::MAX` immortal.
- Alternatives that hold a `STRING` (`STRING OR NULL`, `STRING OR EOF`,
  `STRING OR Error`) use the general layout `{ tag, ptr, i64 }` or a
  sentinel `ptr` (`layout.rs`, `alternatives.rs`, `value-memory-abi.md`
  table “alternative layouts”). A two-word `STRING` changes their payload
  width.
- Vectors use a fat value `{ ptr, i32 }` already (`value-memory-abi.md`);
  their element size for `STRING` changes from 8 to 16 bytes.
- `bn_rt` string functions take one pointer each (`string.rs`); they become
  `(ptr, len)` pairs.
- `ASYNC` isolation: no mutable variable or object is shared with a worker
  (`language/0.6/0.6.md`, “Isolation and conformance”).
- C export (`todo/proposals/c-export.md`) and `BNFfi` (`todo/bucket-0.6.7.md`)
  pass `STRING` as `const char *`.

## Decisions (Carlos, before code)

- [ ] **D1 — Value layout.** Recommended: `{ ptr, i64 }`, where the high bits
  of the second word are flags (`immortal`, `small`, `borrowed`) and the rest
  is `len`. Record the bit positions and the largest `len` they leave.
- [ ] **D2 — SSO.** Yes or no from measurement M. If yes: inline capacity
  (15 bytes on 64-bit targets; wasm32 decided separately), NUL kept inline or
  not, and which operations materialize a heap copy.
- [ ] **D3 — Substrings.** One of: (a) every substring copies (no change);
  (b) slices of immortal strings only (no owner needed); (c) a three-word
  value with the owner. Decided from measurement M.
- [ ] **D4 — Threads.** A `STRING` slot is never written and read by two
  threads at once. Holds by the `ASYNC` isolation rule; since K44 closed, every
  `STATIC` is thread-local and reset per task, so F.4 proves it for
  `STRING` with the two-word value.
- [ ] **D5 — Runtime ABI.** Recommended: `bn_rt` takes a string as two scalar
  arguments `(ptr, i64)` and returns one through an out-pointer, never a
  16-byte struct by value. One rule on SysV, AArch64, Windows x64 and wasm32.
- [ ] **D6 — C boundary.** A `STRING` going to C (HOST syscalls, C export,
  `BNFfi`) is materialized as a NUL-terminated heap buffer when it is small
  or sliced, and passed as is otherwise. Record who frees the copy.

## Activities

### M — Measure first (blocks D2, D3)

- [ ] **M.1** Instrument a measurement build of `bnc` programs (runtime
  counters behind a test-only feature, not a HOST surface). Count:
  allocations by string length (≤ 15, ≤ 23, larger), substring-producing
  operations, retain/release per second.
- [ ] **M.2** Run the 213-program corpus and the benchmarks under
  `tests/`; record the distribution in this bucket. Stage 1 is the baseline.
- [ ] **M.3** Go / no-go per optimization (SSO, slices). If neither pays,
  close the bucket with the measurement and no code change. The two-word
  value alone is not worth its cost.

### S — Specification of the representation

- [ ] **S.1** `value-memory-abi.md`: value layout (D1), ABI rule (D5), C
  boundary (D6), alternative and vector layouts with a two-word `STRING`.
- [ ] **S.2** `support-matrix.md`: wasm32 row (D2 capacity on wasm32).

### F — Fixtures and gates first

- [ ] **F.1** Parity fixtures (`tests/compiler_parity.rs`, exact output) for
  strings of length 0, 1, 15, 16, 23, 24 and large, with and without NUL,
  across concatenation, comparison, indexing, `LEN`, `PRINT`, fields,
  vectors, alternatives, arguments, returns and `ASYNC` arguments/results.
- [ ] **F.2** `tests/ownership_matrix.rs` `STRING` cells pass with the
  stage 1 leak gate (live count unchanged after the loop).
- [ ] **F.3** ABI gate: `value-memory-abi.md` machine-checked list updated;
  no `bn_rt` export returns a struct by value (scan of `runtime_abi.rs`).
- [ ] **F.4** Fixture for D4: `STATIC` `STRING` read and written by `ASYNC`
  workers produces the `bni` output (extend
  `tests/grammar/valid/dispatch-static-isolation.bn`).

### E — Emitter and runtime

- [ ] **E.1** One layout helper in `layout.rs` for the two-word value; every
  load, store, phi, parameter, return, field, element and alternative payload
  goes through it (no second representation).
- [ ] **E.2** `bn_rt` string exports take `(ptr, len)` and return through
  out-pointers (D5); `bin/bn-wasm` matches.
- [ ] **E.3** SSO and/or slices only if M.3 said go.
- [ ] **E.4** C boundary per D6 in HOST calls; C export and `BNFfi` as their
  buckets reach this point.

### C — Close

- [ ] **C.1** Audit (rule of 2026-10-14): allocation/release pairs, `unsafe`
  and `SAFETY:`, tearing (D4), ABI on the three desktop platforms and wasm32.
- [ ] **C.2** Corpus probe unchanged or better; measurement M repeated and
  recorded against the stage 1 baseline.
- [ ] **C.3** `cargo build -p bni -p bnc -p bn_rt`; `cargo test --workspace`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo fmt --check`; `git diff --check`; Windows test host; CI on Linux,
  macOS and Windows.
- [ ] **C.4** `docs/releases/` note; this file to `done/`.

## Not in this bucket

New string operations or slicing syntax (language changes need their own
bucket and spec pass); a tracing GC; mutable strings; rope representations.
