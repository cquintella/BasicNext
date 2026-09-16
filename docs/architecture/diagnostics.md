# Diagnostic data contract

This document defines the diagnostic handoff (0.5.1, registry redesign in
0.5.1a). Identity lives in one table — `REGISTRY: &[DiagDesc]` in
`crates/bn_diag/src/lib.rs` — and user-facing messages in the Fluent shards
under `share/bn/diagnostics/en-US/`. There is no separate emit-site inventory:
the type system and a Rust test gate replace it (see
[Registry and gate](#registry-and-gate)).

## Ownership and flow

The required flow is:

`producer -> DiagId -> DiagnosticSpec -> catalog -> RenderedDiagnostic -> consumer`

- `DiagId` owns stable identity, display code and default severity.
- `DiagnosticSpec` owns typed arguments, effective severity and ordered labels.
- `CatalogEntry` owns title, message, default label text, zero to three causes
  and optional help. Catalog prose cannot change identity, severity or control
  flow.
- `RenderedDiagnostic` is presentation-ready data. CLI and LSP consumers must
  not recover structured facts by parsing its prose.
- Legacy `Diagnostic` remains an explicit compatibility bridge during DX03.
  Unknown legacy codes use the existing renderer; catalog failure must not hide
  the original diagnostic.

## Arguments and labels

Arguments use `DiagnosticValue`, whose supported value kinds are text, signed
integer, unsigned integer and boolean. `DiagId::argument_schema()` defines the
accepted names and kinds. Rendering rejects missing, duplicate, unknown and
wrongly typed values before interpolation. All currently legacy identities use
the explicit `message: Text` compatibility schema; DX03 replaces that schema as
each producer is migrated. A free-form `message` argument is not a completed
producer migration.

Labels retain their complete `bn_source::Span`. A span contains `SourceId`,
`Revision`, byte offset, line and column at both ends. The first primary label
is the diagnostic's deduplication location. Therefore the deduplication key
includes source and revision and cannot merge equal coordinates from different
files or snapshots. Secondary labels may refer to other source identities and
must be resolved by the consumer.

Label text belongs to the catalog unless a producer supplies contextual text.
DX02 completes substitution of catalog primary and secondary defaults.

## Severity

`DiagId::severity()` is the immutable default. Warning policy computes
`DiagnosticSpec::effective_severity` before rendering. Hard errors cannot be
allowed or demoted. Consumers use the effective field directly; changing words
in rendered output is not a severity implementation.

## Separation from BN runtime errors

A BN language `Error`, including the portable `HOST.Exec` error constants, is a
runtime value governed by the language and HOST contracts. It is not a
toolchain `DiagnosticSpec`. A runtime failure may produce a diagnostic at the
host boundary, but registration or rendering must not alter the BN `Error`
value, its code, or catchability.

## Registry and gate

`DiagId` is a validated handle (an index) into `REGISTRY`; every identity is a
`const` on the type (`DiagId::TYPE_MISMATCH`, `DiagId::DIVISION_BY_ZERO`).
Each row carries `code`, `fluent_id`, default `severity` and the typed
argument `schema`. The `diagnostic_registry!` macro generates the handles and
the table from the same list, so an index cannot drift.

There is **no** way to construct an identity outside the table: the former
`DiagId::Runtime(&str)` escape hatch is gone, and every producer helper
(`runtime_error`, `heap_error`, `temporal_error`, semantic `error`) takes a
`DiagId`, not a code string. A typo in a code is a compile error, not a CI
finding. Runtime crates that report codes as strings across the C ABI
(`bn_rt`) are mapped onto registry identities by the interpreter, one variant
at a time, never by string lookup.

**Adding a diagnostic** is one registry row plus one `.ftl` entry whose
`{$arguments}` match the row's schema. Nothing else.

**Gate** (`cargo test -p bn_diag`, always on):

- `registry_matches_pre_refactor_golden` — `tests/registry-golden.tsv` pins
  code / fluent id / severity / schema for every identity; a change to any of
  them is deliberate and updates the golden in the same change.
- `registry_catalog_and_schema_are_mutually_exhaustive` — every row has a
  catalog entry with the same code, every `{$arg}` in that entry is in the
  schema, no orphans in either direction, no duplicate machine code.
- `gate_rejects_*` — negative fixtures prove the catalog loader fails on an
  orphan catalog entry, an uncovered registry row, and a template argument
  outside the schema.

The same checks run at catalog load (`Catalog::from_shards`), so an installed
overlay that drifts is rejected at startup as well.

The Python emit-site inventory (`tests/update_diagnostic_inventory.py`,
`tests/test_diagnostic_inventory.py`, `diagnostics/sites.json`,
`diagnostics/inventory.toml`) was retired in 0.5.1a (D-F8-02); CI no longer
needs Python for diagnostics.

## Supported Fluent subset

Basic Next 0.5.1 intentionally supports a strict offline subset rather than
claiming full Fluent compatibility:

- one message per id using `id = value`;
- indented continuation lines for messages and textual attributes;
- variables written as `{$name}` and checked against the `DiagId` schema;
- `.title` and `.code` required; `.label`, `.label_secondary`, `.help`, and
  `.cause` through `.cause3` optional;
- blank lines and lines beginning with `#` ignored;
- duplicate ids, codes or attributes, unknown ids/attributes/variables,
  malformed lines and multiline `.code` rejected.

Overlay directories point directly at the `en-US` shard directory. Missing
shards and ids inherit the embedded catalog, while an explicitly selected path
that is not a readable directory fails eagerly with `CONFIG_INVALID`, before
source analysis. This bootstrap identity does not depend on the invalid catalog.
Selection order is
`BN_DIAGNOSTICS_DIR`, `[diagnostics].dir` in the selected config, installation
share directory, then embedded `en-US`. Relative `diagnostics.dir` values are
resolved from the selected config file. A CLI invocation loads its selected
catalog once into immutable options; the compatibility process-global loader
also initializes once.

The supported `config.toml` string subset uses double-quoted values with `\\`,
`\"`, `\n`, and `\t` escapes. A `#` begins a comment only outside a quoted
value. Duplicate and unknown keys in `[warnings]`, and duplicate or unknown
keys in `[diagnostics]`, are errors.

T03 must serialize this same `CONFIG_INVALID` bootstrap fact in the locked JSON
v1 envelope. That routing integration is not claimed by DX02 and is an explicit
T03 acceptance dependency; DX02 guarantees the stable identity and message
without consulting the failed catalog.
