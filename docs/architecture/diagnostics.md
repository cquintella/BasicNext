# Diagnostic data contract

This document defines the 0.5.1 diagnostic handoff. Exact emit sites are in the
generated [`diagnostics/sites.json`](../../diagnostics/sites.json); constructor
routes are classified in
[`diagnostics/inventory.toml`](../../diagnostics/inventory.toml). These files are
inventory data, not the message catalog. User-facing messages remain in the
Fluent shards under `share/bn/diagnostics/en-US/`.

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

## Inventory states

Each named route in the inventory is either:

- `structured`: creates a `DiagnosticSpec` with `DiagId` before presentation;
- `legacy`: still creates a free-form `Diagnostic` or prints a diagnostic and
  must be migrated by DX03.

The generator records every recognized emit call with exact path and line,
identity, catalog, consumers and migration state. The gate regenerates it in
memory and rejects any difference, then checks constructor routes, registered
literal codes and catalog paths. Adding or moving a recognized producer makes
the checked-in inventory stale. Dynamic-code constructor routes remain explicit
in the TOML classification. Clearing a route from the DX03 migration requires
changing its state only after its producer and consumer evidence exists.
Direct `eprintln!` producers without a literal stable code are recorded as
`legacy:uncoded` or `legacy:dynamic`, with no fictitious catalog entry; the
emitting file is also their current rendering consumer.

Regenerate intentionally with `python3 tests/update_diagnostic_inventory.py`;
CI uses its `--check` mode.

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
