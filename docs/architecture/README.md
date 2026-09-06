# Architecture (target)

Target toolchain architecture for BasicNext: minimalist language,
frontend → IR → interpret(IR) | compile(IR), optional `bnc`, external LLVM.

Standing agent/contributor brief (authority + acceptance, including well-formed IR handoff **W1–W5**): [`../../AGENTS.md`](../../AGENTS.md).

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
| [module-path.md](module-path.md) | Ordered **module-path** (dirs to search for `.bn` imports) |
| [semantic-analysis.md](semantic-analysis.md) | **2.5** contract: definitions, operands, booleans, calls/returns, references |
| [conformance.md](conformance.md) | Spec → reference interpret → compile; two CI comparisons |
| [value-memory-abi.md](value-memory-abi.md) | Value/memory/ABI + numeric lowering (`nsw` ≠ BN overflow) |
| [frontend-session.md](frontend-session.md) | `FrontendSession`: snapshots, SourceId/Revision, check≡LSP diagnostics |
| [completion-gates.md](completion-gates.md) | Correctness gates (**GC-***); priority sequence; DAG+smoke ≠ enough |
| [review-status.md](review-status.md) | Honest review table: design met vs partial vs insufficient |
| [to-close.md](to-close.md) | **Checklist of what remains to close** each contract |
| [ownership.md](ownership.md) | 0.4.4 current FE/IR/BE/EDGE ownership inventory and debt |
| [driver-sequence.md](driver-sequence.md) | 0.4.4 as-is CLI pipeline and LSP/DAP divergence inventory |

## Deeper design (P2 companions)

| Path | Role |
|------|------|
| [sequences.md](sequences.md) | Mermaid sequences for check / interpret / compile / LSP / DAP |
| [host-traits.md](host-traits.md) | HostEnv, Capabilities, provider traits sketch (2026-09-05) |
| [nfr-security.md](nfr-security.md) | NFR/security architecture index (points to threat model + register) |
| [open-questions.md](open-questions.md) | Open architecture decisions register |
| [glossary.md](glossary.md) | Architecture glossary |
| [support-matrix.md](support-matrix.md) | Support matrix **contract** (structured + verifiable); data still stub/EXAMPLE |

## Language specification (companion — not DFD)

Toolchain architecture **consumes** the language; it does not replace the grammar.

| Path | Role |
|------|------|
| [../language/0.4/0.4.ebnf](../language/0.4/0.4.ebnf) | Normative **EBNF** syntax (0.4) |
| [../language/0.4/0.4.md](../language/0.4/0.4.md) | Static semantics, runtime behaviour, diagnostics — **active draft** (0.3 incorporated; not G0-accepted final) |
| [../language/0.4/keywords.md](../language/0.4/keywords.md) | Keywords |

DFD **2.0 Analyze Sources**: lex/parse must match the EBNF; **2.5** must satisfy [semantic-analysis.md](semantic-analysis.md).

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
| One IR; interpret = **executable reference** subordinate to the **spec** (not `lli`); LLVM equivalent on support subset; conformance = spec fixtures + cross-backend | **Locked** |
| Same Frontend→IR for CLI and IDE | **Locked** |
| `bnc` UX: default interpret / `-c` / `--target` / `--check` | **Locked** |
| Companion process log + `--log-level`; plugins-dir reserved | **Locked (MVP)** |
| **Module search path** = ordered list (`--module-path` repeatable) | **Locked (direction 2026-09-05)** |
| **Semantic analysis** must cover definitions-by-path, operand compatibility, boolean conditions, call/return signatures, valid references | **Locked (2026-09-05)** |
| **`bn_ir` independent of frontend** — lowering in frontend; IR = types/ops/validate only; backends consume IR without semantic analyzer | **Locked (2026-09-05)** |
| **Value/memory/ABI contract** required for interpret↔compile equivalence (beyond `bn_value` extract); LLVM poison/`nsw` ≠ BN `Error` | **Locked (2026-09-05)** |
| **HOST: program requirements ≠ target support ≠ execution policy**; deny ≠ unimplemented; `bn_rt` enforces policy at runtime | **Locked (2026-09-05)** |
| **Support matrix** = structured verifiable catalog; `validate` ≠ target-support check; parity gates beyond stdout | **Locked (2026-09-05)**; **data inventory still open** |
| Ship `bnc` binary in 0.4.5 | **Optional / proposed** |
| IDE subscription to pipeline events | **Future** |
| Fluent catalogs for `bn_diag` | **Chosen for 0.4.5** (proposal); wire into crates in XM6 |

## Related

- Pre-refactor implementation: `ongoing/bucket-0.4.4.md`
- Refactor + diagnostics implementation: `ongoing/bucket-0.4.5.md`
- As-is audit DFDs (local): `audit/workpapers/09-synthesis/dfd-*-as-is.md`
