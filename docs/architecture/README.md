# Architecture (target)

Target toolchain architecture for BasicNext: minimalist language,
frontend → IR → interpret(IR) | compile(IR), optional `bnc`, external LLVM.

## Documents

| Path | Role |
|------|------|
| [target-architecture.md](target-architecture.md) | Target design (DAG, principles, split, migration **XM***) |
| [milestones-map.md](milestones-map.md) | Crosswalk **SM*** / **XM*** / release buckets |
| [dfd/dfd-0-to-be.md](dfd/dfd-0-to-be.md) | DFD-0 context |
| [dfd/dfd-1-to-be.md](dfd/dfd-1-to-be.md) | DFD-1 named flows |
| [dfd/dfd-2/README.md](dfd/dfd-2/README.md) | DFD-2 index (one file per process) |
| [dfd/data-dictionary.md](dfd/data-dictionary.md) | Data dictionary (flows, elements, stores) |
| [ir-contract.md](ir-contract.md) | BN IR contract (to-be) — stub |

## Deeper design (P2 companions)

| Path | Role |
|------|------|
| [sequences.md](sequences.md) | Mermaid sequences for check / interpret / compile / LSP / DAP |
| [host-traits.md](host-traits.md) | HostEnv, Capabilities, provider traits sketch (2026-09-05) |
| [nfr-security.md](nfr-security.md) | NFR/security architecture index (points to threat model + register) |
| [open-questions.md](open-questions.md) | Open architecture decisions register |
| [glossary.md](glossary.md) | Architecture glossary |
| [support-matrix.md](support-matrix.md) | Interpret × LLVM support matrix — **stub** (EXAMPLE rows only) |

## Language specification (companion — not DFD)

Toolchain architecture **consumes** the language; it does not replace the grammar.

| Path | Role |
|------|------|
| [../language/0.4/0.4.ebnf](../language/0.4/0.4.ebnf) | Normative **EBNF** syntax (0.4) |
| [../language/0.4/0.4.md](../language/0.4/0.4.md) | Static semantics, runtime behaviour, diagnostics |
| [../language/0.4/keywords.md](../language/0.4/keywords.md) | Keywords |

DFD **2.0 Analyze Sources** (lex/parse) must match this EBNF.

## Decisions & proposals (linked)

| Topic | Where |
|------|------|
| Soft→hard split (**SM0–SM7**) | [`../../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md) |
| `bnc` locks | [`../../audit/workpapers/09-synthesis/bnc-decisions.md`](../../audit/workpapers/09-synthesis/bnc-decisions.md) |
| `bnc` options surface | [`../../audit/workpapers/09-synthesis/bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md) |
| Process-log critique (non-MVP leftovers) | [`../../audit/workpapers/09-synthesis/pipeline-observability-critique.md`](../../audit/workpapers/09-synthesis/pipeline-observability-critique.md) |
| Fluent / expressive diagnostics | [`../../todo/proposals/expressive-diagnostics.md`](../../todo/proposals/expressive-diagnostics.md) |

### Locked vs proposed (short)

| Item | Status |
|------|--------|
| Minimalist language posture | **Locked** |
| One IR; interpret = oracle (not `lli`); LLVM tools external | **Locked** |
| Same Frontend→IR for CLI and IDE | **Locked** |
| `bnc` UX: default interpret / `-c` / `--target` / `--check` | **Locked** |
| Companion process log + `--log-level`; plugins-dir reserved | **Locked (MVP)** |
| Ship `bnc` binary in 0.4.5 | **Optional / proposed** |
| IDE subscription to pipeline events | **Future** |
| Fluent catalogs for `bn_diag` | **Chosen for 0.4.5** (proposal); wire into crates in XM6 |

## Related

- Pre-refactor implementation: `ongoing/bucket-0.4.4.md`
- Refactor + diagnostics implementation: `ongoing/bucket-0.4.5.md`
- As-is audit DFDs (local): `audit/workpapers/09-synthesis/dfd-*-as-is.md`
