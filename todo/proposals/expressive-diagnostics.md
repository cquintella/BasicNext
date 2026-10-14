# Proposal: Expressive diagnostics (Fluent catalog)

## Planning review addendum (2026-09-13)

Execution detail is in [bucket 0.5.1](../../ongoing/bucket-0.5.1.md), SECTION 2,
and [WBS 0.5.1](../../ongoing/WBS-0.5.1.md). Retain the locked Fluent catalog
and TOML configuration split. No catalog database or language Error redesign.

The "What exists today" paragraph below predates the current implementation.
At f675470, bn_diag already contains DiagId, DiagnosticSpec, DiagnosticSink,
RenderedDiagnostic, embedded shards, overlays and warning policy. CLI omits
causes/help; catalog rendering does not apply label defaults; LSP already
appends causes/help but needs effective-policy and cross-source label fixes.
Extend these paths rather than introducing parallel infrastructure.

D-DX-01 is resolved by Carlos (2026-09-13), option B: 0.5.1 requires complete
migration of every user-visible diagnostic producer in lexer, parser, semantic/
frontend, lower/IR validation, interpreter/runtime, target support/LLVM, CLI/bnc
and LSP. DX01–DX05 govern execution. The explicit twelve-case DX03 corpus is the
mandatory regression floor, not the migration boundary. Catalog completeness
alone does not prove producer coverage or useful advice. Generic `{$message}`
forwarding does not count. Any unavoidable bootstrap/internal exception must be
explicitly inventoried, justified and tested.

The handwritten parser accepts only a subset of Fluent and rejects the multiline
example below. DX02 must reconcile this before claiming authoring support:
document/validate the actual subset or support the documented syntax. Validate
argument contracts, duplicate attributes and malformed config as well as ids.
Treat the AS conversion help below as illustrative pending language review;
all shipped correction examples must pass the 0.5.0 language check.

---

**Status:** Proposed for **0.5.1** (toolchain / diagnostics — not language DNA).
**Retarget (Carlos 2026-09-13 via Quorra):** was **0.4.5** (refactored release; largely skipped); now tracked in `ongoing/bucket-0.5.1.md` as item **DX1**, orthogonal to H1 (`HOST.Exec`) and T1 (`bn eval`). Does not block or alter 0.5.0 ARC.
Decisions 2026-09-04 (still locked): catalog is **Fluent** (`.ftl`); **warnings** first-class;
**no SQLite**; prose out of call sites; architecture review applied.
**Locked final (2026-09-04):** catalog **sharded by pipeline from day one**;
warning policy is **per-code in `config.toml` + global promote**; ship
**embedded `en-US` + optional overlay dir**; **English-only content first**,
locale directory layout ready for future translations.

**Premise:** Basic Next should feel like rustc at the moment of failure:
one glance at *what*, one sentence of *why it failed here*, a short list of
*likely causes*, and a single high-value *what to try*.

## What exists today

`Diagnostic` is three fields (`code`, `message`, `span`). CLI render is a
single caret. No notes/causes/help. Call sites own prose. Lexical codes use
`E0001`; semantic/runtime use `SNAKE_CASE`. LSP maps only code + message.

The BN value `Error` (`Code AS INTEGER`, `Message AS STRING`) is a separate
channel for expected `T OR Error` failures. This proposal does not change
that surface and adds no keywords.

## Precedent (rustc, not Cargo)

Cargo itself largely embeds CLI strings. **rustc** keeps diagnostic prose in
Fluent resources: per-crate `messages.ftl`, message id + attributes
(`.label`, `.help`), variables `{$name}`, typed ids via `fluent_messages!`,
optional `$sysroot/share/locale/<locale>/*.ftl`.

BN copies that shape with a smaller attribute set (title / causes / help) and
the scalability rules in [Architecture](#architecture-optimization--scalability).

## Goals

1. Every diagnostic (error **or** warning) has a stable **code**, a short
   **title**, a one-sentence **message**, span **labels**, up to three
   **causes**, and at most one **help**.
2. User-facing prose lives in Fluent `.ftl` files, not in emit-site literals.
3. CLI and LSP share one model; formatting is lazy and cheap to drop.
4. Cognitive load stays low (0–3 causes).
5. Cost scales with *emitted* diagnostics, not with catalog size on the hot
   path after a single process-wide load.

## Non-goals

- No `try/catch`, stack traces in BN, or BN `Error.Causes` field.
- No SQLite / other DB for the catalog.
- No TOML catalog (Fluent wins for rustc parity and attributes).
- Ship only `en-US` prose in 0.5.1; do not translate yet. The on-disk /
  embedded layout must already be locale-keyed so adding `pt-BR` (etc.) is
  copy-files + select locale, not a redesign.
- No full-text search engine inside `bn check`.

---

## Architecture (optimization / scalability)

### Decisions from review

| Topic | Decision | Why |
| --- | --- | --- |
| Hot path | Emit stores **id + args + spans** (`DiagnosticSpec`); Fluent runs only at **render** / LSP publish | Warnings may be filtered (`allow`) or deduped before any formatting cost |
| Catalog load | **Once** per process (`OnceLock`); parse `.ftl` → internal `Catalog` map | Do not re-parse Fluent or re-walk attributes on every emit |
| Severity / policy | Live in a **Rust registry** keyed by id (`Severity`, default allow, future lint level) | Translators must not change exit codes by editing `.severity` in prose files |
| Display code | Explicit `.code` in `.ftl` **or** fixed kebab→`SCREAMING_SNAKE` in registry | One mapping, O(1); no string heuristics at render |
| Typed ids | `enum DiagId` or `fluent_messages!`-style constants after migration | Avoid typo’d `"numeric-overlow"` strings; enables exhaustiveness |
| Dedup | `(DiagId, primary Span)` set before render | Analysis can be noisy; rustc does this |
| Memory | Intern display codes/`DiagId`; formatted strings are owned only after render | Hundreds of warnings must not clone catalog templates |
| Bundle sync | Immutable catalog after load; LSP single-threaded today | No lock on emit; safe to share `&Catalog` |
| FS vs embed | **Final:** embed `en-US` shards via `include_str!`; **overlay dir** replaces ids when present (`BN_DIAGNOSTICS_DIR` / `config` / `share/bn/diagnostics`) | Offline default + packager/translator path without redesign |
| Sharding | **Final:** shard by pipeline from day one (`lex`, `parse`, `sem`, `runtime`, `build`, `lsp`); loader merges into one `Catalog` | >100 messages certain; avoids perpetual “split later”; diffs stay local |
| Dependencies | Prefer thin Fluent parse-once into BN structs over calling `format_pattern` six times per diagnostic | Attribute fan-out (title, label, cause×3, help) must stay O(patterns × args) once |

### Process data flow

```text
emit(DiagId, Args, Spans)
  → push DiagnosticSpec into DiagnosticSink
  → filter allow / -Werror / dedup
  → render(spec, &Catalog) → Diagnostic (formatted) → CLI | LSP
```

`DiagnosticSink` is the scalability boundary: check/build collect; run aborts
on first **error** after formatting that one (or formats all warnings at end).

### Load path (resolution order)

1. `BN_DIAGNOSTICS_DIR` if set (test / packagers).
2. `diagnostics.dir` in `config.toml` if set.
3. Directory beside the executable: `share/bn/diagnostics/<locale>/`.
4. Embedded `en-US` bytes compiled into `bn`.

Overlay files **replace by message id**, they do not require shipping a full
fork of the catalog. Missing overlay entry falls back to embedded.

### What must not live in `.ftl`

- Severity / lint level (exit behaviour).
- Whether a diagnostic is suppressible.
- Machine code identity used in tests (`NUMERIC_OVERFLOW`) — either registry
  or mandatory `.code` validated at catalog load.

Prose-only attributes stay in Fluent: primary text, `.title`, `.label`,
`.label_secondary`, `.cause` / `.cause2` / `.cause3`, `.help`.

### Scale envelopes (targets)

| Envelope | Target |
| --- | --- |
| Catalog size | 10²–10³ messages, still one load &lt; few ms |
| Emit | ≥10⁵ specs/sec possible in theory; BN needs correct &gt; max throughput |
| Render | pay Fluent only for diagnostics that survive filtering |
| LSP session | one catalog; republish on doc change without reload unless mtime overlay |

---

## Model

### Core types

```rust
/// Unformatted emission (cheap). Not yet user-visible.
pub struct DiagnosticSpec {
    pub id: DiagId,
    pub args: DiagArgs,           // smallvec of (name, DiagArgValue)
    pub labels: Vec<Label>,       // spans; optional override label text
}

/// After render — ready for CLI/LSP.
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Arc<str>,           // interned display code
    pub title: String,
    pub message: String,
    pub labels: Vec<Label>,
    pub causes: Vec<String>,
    pub help: Option<String>,
}

pub enum Severity { Error, Warning }

pub struct Label {
    pub span: Span,
    pub style: LabelStyle,
    pub text: Option<String>,
}

pub enum LabelStyle { Primary, Secondary }

/// Stable emit key. Maps to Fluent id + registry policy.
pub enum DiagId { /* NumericOverflow, TypeMismatch, … */ }
```

### Fluent file format (source of truth for prose)

**Final layout — locale → pipeline shards** (not a single growing file):

```text
share/bn/diagnostics/
  en-US/                 # only locale shipped in 0.4.5 (embedded too)
    lex.ftl
    parse.ftl
    sem.ftl
    runtime.ftl
    build.ftl
    lsp.ftl
  # pt-BR/               # future: same shard names, translated attributes
  #   lex.ftl
  #   ...
```

Each `en-US` shard is `include_str!` into `bn`. Loader picks a locale, then
concatenates shards in fixed order into one `Catalog`. Message **ids** are
locale-invariant (same `numeric-overflow` in every language); only attribute
prose changes. CI fails on duplicate ids within a locale. New frontend areas
get a new shard file under every locale that exists.

**Locale selection (structure now, English default):**

1. CLI `--locale <tag>` if present (future UX; may land with the catalog).
2. `diagnostics.locale` in `config.toml` if set.
3. Else `en-US`.

If a requested locale is missing, fall back to embedded `en-US` and emit one
toolchain note (not a BN program error). Never mix shards from two locales in
one `Catalog`.

```fluent
## Integer overflow when a checked op exceeds the destination type.
numeric-overflow =
    {$type} cannot hold {$value}
    .title = Integer overflow
    .code = NUMERIC_OVERFLOW
    .label = overflow here
    .label_secondary = declared here
    .cause = the left operand is already at the maximum for {$type}
    .cause2 = the operation increased the magnitude
    .help = widen the binding or reduce the operand

unused-binding =
    `{$name}` is never read
    .title = Unused binding
    .code = UNUSED_BINDING
    .label = never read
    .cause = the binding was kept for a side effect that is gone
    .cause2 = the name is misspelled at the use site
    .help = remove the binding or rename it if retention is intentional

type-mismatch =
    cannot {$op} `{$left}` and `{$right}`
    .title = Type mismatch
    .code = TYPE_MISMATCH
    .label = here
    .cause = the operands come from declarations of different types
    .cause2 = an explicit `AS` conversion is missing
    .help = insert `AS {$right}` or change a binding type
```

| Piece | Where | Notes |
| --- | --- | --- |
| Fluent id | `numeric-overflow` | kebab-case; matches `DiagId` |
| Display code | `.code` | required; load fails if missing/duplicate |
| Primary text | message value | one sentence |
| Title | `.title` | required |
| Labels | `.label`, `.label_secondary` | defaults; emitter may override |
| Causes | `.cause` … `.cause3` | 0–3 |
| Help | `.help` | at most one |
| Severity | **Rust registry** | not in `.ftl` |

### Emission

```rust
sink.emit(
    DiagId::NumericOverflow,
    args![("type", "INT8"), ("value", "128")],
    labels![primary(expr_span), secondary(binding_span)],
);
```

Missing catalog row for a `DiagId` → hard `INTERNAL` at render (tests must
catch at CI by walking `DiagId` vs catalog).

### Render (CLI)

```text
error[NUMERIC_OVERFLOW]: Integer overflow
  --> file.bn:328:9
   |
328 |         value += 1
   |         ^^^^^ overflow here
   |
324 |     LET value AS INT8 = 127
   |         ----- declared here
   |
   = message: INT8 cannot hold 128
   = cause: the left operand is already at the maximum for INT8
   = cause: the operation increased the magnitude
   = help: widen the binding or reduce the operand

warning[UNUSED_BINDING]: Unused binding
  --> file.bn:10:9
   |
10 |     LET tmp AS INTEGER = 1
   |         --- never read
   |
   = message: `tmp` is never read
   = help: remove the binding or rename it if retention is intentional
```

### LSP

| Field | LSP |
| --- | --- |
| severity | from registry |
| code | display code |
| range | primary span |
| message | `{title}: {message}` + causes/help text for stock clients |
| secondary | `relatedInformation` |
| extras | optional `data` for the VS Code extension |

---

## Warnings (final policy)

Same Fluent prose and `DiagnosticSpec` path. **Default severity** comes from
the Rust registry (`Error` vs `Warning`). **Effective level** is registry
overlaid by config, then by CLI.

### Levels (per display code)

| Level | Meaning |
| --- | --- |
| `allow` | drop in the sink (no render) |
| `warn` | print as `warning[CODE]`; exit 0 unless errors remain |
| `error` | print as `error[CODE]`; non-zero exit |

Hard registry `Error` (e.g. `TYPE_MISMATCH`, `NUMERIC_OVERFLOW`) **cannot** be
`allow`’d — config that tries is a tool error. Only ids marked `warnings_allowed`
in the registry accept `allow` / demotion.

### `config.toml` (normative for 0.5.1)

```toml
[warnings]
# optional default for all warning-class ids: warn | error
default = "warn"

[warnings.levels]
UNUSED_BINDING = "allow"
UNUSED_IMPORT = "warn"
UNREACHABLE_CODE = "error"
```

### CLI

| Flag | Effect |
| --- | --- |
| `--warnings errors` | promote every warning-class id to `error` for this run (after config) |
| `--allow CODE` | same as config `allow` for one code (repeatable) |
| `--deny CODE` | force `error` for one warning-class code (repeatable) |
| `--warn CODE` | force `warn` for one warning-class code (repeatable) |

Precedence (high wins): CLI `--allow/--deny/--warn` → `--warnings errors` →
`[warnings.levels]` → `[warnings].default` → registry default.

No per-line source pragmas in 0.5.1 (DNA). Dedup `(DiagId, primary span)`
applies before render. `bn run`: analysis warnings follow the same levels;
they never skip `Start` unless effective level is `error`.

First warning-class ids: `UNUSED_BINDING`, `UNUSED_IMPORT`, `UNREACHABLE_CODE`.

---

## Loader

1. Resolve locale (default `en-US`). Load that locale’s shards in fixed order
   (embedded `en-US`, then overlay files of the same relative paths if
   present). Overlay **replaces by Fluent id** within the locale. Other
   locales are directories beside `en-US/`; they are not loaded until selected.
2. Parse **once** into `HashMap<DiagId, CatalogEntry>` (pre-split patterns).
3. Validate: every `DiagId` has an entry; every `.code` unique across shards;
   required `.title` present; no id in two shards.
4. Expose `&Catalog` through `OnceLock`.
5. Author search = ripgrep on `en-US/*.ftl`. Updates = git edit. No DB.

---

## Implementation phases

### A — structure

`DiagnosticSpec` + `DiagnosticSink` + new `render`. Compatibility wrapper for
old `error(code, message, span)`.

### B — Fluent catalog + registry

1. Embedded `en-US/{lex,parse,sem,runtime,build,lsp}.ftl` + loader/validator.
2. Registry: `DiagId` → severity (+ display code cross-check).
3. Migrate priority codes; golden snapshots for error and warning.
4. CI: `DiagId` exhaustiveness vs catalog.

### C — secondary spans + dedup

Type mismatch, overflow, duplicate name; sink dedup.

### D — docs / overlay

Optional diagnostics dir; generated docs from `.ftl`; LSP `codeDescription`.

Acceptance: priority codes render with title + message + cause or help;
surviving diagnostics only pay Fluent format; severity unchanged if `.ftl` is
hand-edited wrongly on a non-severity field; at least one warning; no BN
grammar change.

---

## Alternatives considered

- **Eager format at emit** — rejected: wastes work on filtered warnings.
- **Severity inside `.ftl`** — rejected: policy must not be localizable.
- **TOML catalog** — rejected: Fluent attributes match rustc better.
- **SQLite** — rejected: heavy, poor diffs.
- **Stringly emit ids forever** — rejected at scale; migrate to `DiagId`.
- **BN `Error.Causes`** — outside this 0.5.1 toolchain change.
- **AI help** — rejected.

## Decisions locked (2026-09-04)

1. **Shards from day one** — `lex` / `parse` / `sem` / `runtime` / `build` /
   `lsp` `.ftl` files; merge at load; globally unique ids.
2. **Warning policy final** — per-code levels in `config.toml`
   (`allow`/`warn`/`error`) + CLI `--allow`/`--deny`/`--warn` +
   `--warnings errors`; hard errors not suppressible.
3. **Embedded + overlay** — `include_str!` fallback always; optional
   diagnostics dir overrides by id (packagers) without a second design pass.
4. **i18n-ready, English-only ship** — tree is `diagnostics/<locale>/*.ftl`;
   0.5.1 ships only `en-US`. Future languages = add a sibling locale folder
   with the same shard names and ids; selection via config/CLI. No redesign.

## Changelog

- **2026-09-04** — Locked Fluent / shards / warnings / no SQLite; target then 0.4.5.
- **2026-09-13** — Carlos: retarget to **0.5.1** (bucket DX1); was idle after 0.4.5 window.
- **2026-09-13** — Carlos: D-DX-01 option B — complete user-visible diagnostic producer migration; twelve-case corpus is the regression floor.
