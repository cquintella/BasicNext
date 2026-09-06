# FrontendSession — snapshots, source identity, diagnostics (to-be)

> Canonical: `docs/architecture/frontend-session.md`  
> **Status:** **direction locked 2026-09-05**; API details open (AQ-18).  
> Complements principle 2 (*shared frontend → IR*) in [target-architecture.md](target-architecture.md).

---

## Why `open(path)` + model cache is not enough

`FrontendSession` is the right shared door for CLI, LSP, and DAP. A design that only exposes `open(path)` and caches analyzed models **fails the IDE** because editors routinely present:

- **unsaved buffers** (URI / virtual text that is not yet on disk);
- **revisions** (every keystroke or applyEdit changes the text under the same path);
- **dependent invalidation** (editing A may stale B that imported A);
- **cancellation** (a newer edit must abort an in-flight analyze);
- **diagnostics publication** tied to the **revision that produced them** (never paint squiggles from an older analysis onto a newer buffer).

Without explicit rules for those, “same Frontend as CLI” is aspirational, not contractual.

---

## Source identity and revision (required)

### As-is gap

Today `Span` in `src/source.rs` carries only `start`/`end` **positions** — no file identity. The IR aggregates multiple units and keeps a module-level `source_name` (`ir/model`, lowering from AST). Positions without a stable **source id** cannot reliably map instructions, diagnostics, or breakpoints back to the correct file after multi-module lowering or transforms.

### To-be rule

Associate every span (and every IR debug/location metadata preserved through lowering) with:

| Concept | Meaning |
| --- | --- |
| **`SourceId`** | Stable identity of a compilation unit / buffer (path URI, untitled buffer id, or equivalent) — **not** only a display string |
| **`Revision`** | Monotonic (or content-hash) id for the **text** of that source at analysis time |
| **`Span`** | Positions **plus** `SourceId` (and, where published to IDE, the `Revision` of the snapshot that was analyzed) |

Lowering **must preserve** `SourceId` (and enough location info) on IR so backends and DAP can relate ops/errors/breakpoints to the correct file **after** aggregation and transforms. A lone aggregated `source_name` on the root module is **not** sufficient for multi-unit programs.

**Locked (2026-09-05):** `SourceId`, `Revision`, and `Span` live in a shared **`bn_source` leaf crate** (or equivalent leaf module cut **below** frontend). **`bn_frontend`**, **`bn_diag`**, and **`bn_ir`** all depend on `bn_source` — **never** the reverse, and **never** place these types only inside `bn_frontend` (that would force `bn_ir`/`bn_diag` → frontend and recreate the forbidden edge). AQ-04 is closed in **direction**; only packaging details (exact crate name/path) remain trivial.

---

## Session model (required capabilities)

`FrontendSession` must support at least:

1. **Upsert snapshot** — bind `(SourceId, Revision, text)` for disk files **and** unsaved buffers (LSP `textDocument/didOpen|didChange`).
2. **Analyze / lower request** — run shared 2.0→3.0 (or a documented subset — see diagnostic classes below) against a **snapshot set**, not “whatever is on disk now.”
3. **Invalidation** — when revision *R* of *S* changes, mark *S* and **dependents** (importers / graph edges) stale; do not serve stale `SemanticModel` / IR as current.
4. **Cancellation** — a newer revision or explicit cancel aborts or discards results from older work; results must carry the revision they belong to.
5. **Publish diagnostics** — only publish to a client for `(SourceId, Revision)` when that revision is still the client’s current buffer (or the protocol’s equivalent freshness check). Stale results are dropped or queued for the matching revision, never applied blindly.
6. **CLI path** — `check` / `run` / `build` load snapshots from the filesystem (revision = file content at open); same session APIs, different snapshot source.

`FrontendSession::open(path)` remains a convenience for CLI; it is **not** the full IDE contract.

---

## Diagnostic classes vs operations (check ≡ LSP promise)

To promise **equivalent diagnostics**, define **what each operation must produce** and **when** Lower/Validate runs.

| Operation | Lex / parse / semantic (2.2–2.5) | Lower + **language** `validate` (3.0) | Target-support check | Notes |
| --- | --- | --- | --- | --- |
| **`bnc --check` / `bn check`** | **Required** | **Required** | No (unless explicitly checking a compile profile) | Full Frontend; see [sequences.md](sequences.md) check profile |
| **LSP “Problems” / publishDiagnostics (default)** | **Required** | **Required for equivalence with `--check`** | No | Same diagnostic *facts* as check for the same snapshot set ([data-dictionary F36](dfd/data-dictionary.md)) |
| **LSP IR-aware features** (if any beyond check-equivalent squiggles) | Required | Required when the feature needs IR | As needed | Must not invent a weaker checker for the default Problems panel |
| **`run` / interpret** | Required | Required | interpret support matrix | |
| **`build` / compile** | Required | Required | **Yes** — `validate_for(target)` | Support rejections ≠ language errors ([support-matrix.md](support-matrix.md)) |

### Resolving the sequences inconsistency

Previously: **check** always showed Analyze → Lower/Validate, while the LSP diagram said Lower “**as needed**.” That undercuts “squiggles match `bnc --check`.”

**Locked:** default IDE diagnostics publication uses the **same Frontend stages as `--check`** (through language `validate`). “As needed” applies only to **extra** IR-consuming IDE features, not to the baseline Problems/`publishDiagnostics` path. [sequences.md](sequences.md) must match this table.

---

## Relationship to other docs

| Doc | Link |
| --- | --- |
| Shared frontend principle | [target-architecture.md](target-architecture.md) § Principles / Contracts |
| Check vs LSP sequences | [sequences.md](sequences.md) |
| Semantic obligations before IR | [semantic-analysis.md](semantic-analysis.md) |
| `validate` vs support | [ir-contract.md](ir-contract.md), [support-matrix.md](support-matrix.md) |
| `bn_source` placement | [open-questions.md](open-questions.md) AQ-04 |
| XM4 | [milestones-map.md](milestones-map.md) |

---

## Non-goals (this document)

- Exact Rust method names beyond the capability list.
- Full LSP protocol mapping (didChange debounce intervals, etc.).
- Replacing Fluent / `bn_diag` catalog design.

## See also

- [glossary.md](glossary.md) — SourceId, Revision, FrontendSession


## `bn_source` DAG (locked)

```text
bn_source  (SourceId, Revision, Span, SourceFile text helpers)
    ↑
    ├── bn_diag
    ├── bn_ir
    └── bn_frontend  (FrontendSession, lex/parse/semantic/lower)
```

Session APIs may *produce* revisions; they do not *own* the identity types.
