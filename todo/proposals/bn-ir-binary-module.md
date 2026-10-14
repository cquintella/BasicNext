# Proposal: binary BN IR modules (the Swift model)

**Status:** Direction decided by the BDFL on 2026-10-12; design points below
to decide before a bucket opens. Not part of 0.6.5.
**Date:** 2026-10-12.
**Target release:** TODO (Carlos decides; after 0.6.5, which closes T.5).
**Surface:** toolchain only (a file format, `bni` / `bnc` options, the module
search). No grammar, keyword or semantic change.

## Decision (Carlos, 2026-10-12)

Basic Next follows the architecture of Swift:

```text
source → frontend → BN IR (typed, validated) ─┬→ bni (interpret)
                                              ├→ bnc: in-memory LLVM module → .ll / object
                                              └→ binary BN IR module (this proposal)
```

* **BN IR is the language's own typed IR**, as SIL is Swift's (Go: the
  unified IR; Rust: MIR). It already exists and both backends consume it
  (W1–W5).
* **`bnc` builds the LLVM module in memory** and renders it at the end
  (bucket 0.6.5, T.5). That is the prerequisite for any binary LLVM output
  and is decided separately.
* **A compiled module is stored as serialized BN IR**, as Swift stores a
  `.swiftmodule` (Go: export data; Rust: `.rmeta`): an importer reads the
  compiled module instead of re-reading and re-analysing its source.
* **It must work interpreted as well as compiled** (Carlos): `bni` and `bnc`
  load the same file. This follows from W3 — the validated BN IR is already
  the one artifact both backends share.

## Motivation

* Imports through `--module-path` re-lex, re-parse and re-analyse every
  module on every run; a compiled module skips that.
* Distributing a module without its source (a library) needs a format.
* `ir-contract.md` ("Versioning") and `target-architecture.md` (`ir_version`
  when serialized) already name a serialized schema as future work.

## Fixed constraints (not open)

1. **Validate on load (W1).** A loaded module is untrusted input: it passes
   the same `bn_ir::validate` as freshly lowered IR before `bni` runs it or
   `bnc` emits it; a malformed file is a diagnostic, never a crash or a
   different meaning.
2. **One artifact (W3).** `bni` and `bnc` read the same bytes; there is no
   interpret-only or compile-only variant.
3. **One shared library (Carlos, 2026-10-12).** The writer, the reader and
   the load-time validation live once, in the shared crate of the IR domain
   (`bn_ir`, or a crate beside it that depends only on `bn_ir`); `bni` and
   `bnc` call it and hold no serialization code of their own (the rule
   "shared libs serve both backends"). It depends on no frontend type (W5).
4. **Deterministic.** The same source and toolchain give the same bytes (no
   `HashMap` order, no timestamps in the payload).
5. **Spans survive.** `SourceId` and spans are kept, so diagnostics, traps
   and `bnc -g` from a compiled module still point at source lines.
6. **HOST policy is not trusted from the file.** Capability requirements are
   recorded, but execution policy is decided where the program runs (the
   rule of `host-traits.md`: requirements ≠ policy).

## Libraries (checked 2026-10-12)

* **Encoding: `serde` (already in the workspace) + `postcard`.** Compact,
  with a stable documented wire format; RUSTSEC-2025-0141 lists it among the
  replacements for `bincode`. `rkyv` only if zero-copy loading is ever
  needed.
* **Not `bincode`:** unmaintained since 2025-12-16 (RUSTSEC-2025-0141).
* **Determinism needs a model change:** `bn_ir/src/model.rs` keeps eight
  `HashSet<ModuleId>` provider sets and `weak_symbols: HashSet<SymbolId>`;
  they become `BTreeSet` (ordered ids) before any byte is written.

## Open points (Carlos decides)

1. **Unit.** One file per BN module (as Swift and Go do per module /
   package), or one file for a whole program?
2. **Name and extension** (e.g. `.bnm`); and whether the contents are only
   the public interface plus inlinable bodies (Swift) or every function.
3. **Compatibility.** Valid only for the exact toolchain release that wrote
   it (Swift's choice for non-library-evolution modules; simplest), or a
   versioned schema with a compatibility promise?
4. **Staleness.** The file records the hashes of its sources and of the
   modules it imports; a mismatch is rejected (or rebuilt from source when
   the source is on the module path?).
5. **Search order.** When both `m.bn` and `m.bnm` are on the module path,
   which wins?
6. **CLI.** How `bnc` writes one (`--emit module`?) and how `bni` runs or
   imports one (`bni run m.bnm`?), matching the existing option owners (K3).
7. **Integrity.** A checksum only, or a signature for distributed modules?

## Verification (when accepted)

Round trip (lower → write → read → validate → equal model) for the whole
corpus; malformed and truncated files rejected with a stable diagnostic
(fuzzed); parity `bni` × `bnc` on programs importing compiled modules;
deterministic bytes (two writes, same hash); the book and man pages.
