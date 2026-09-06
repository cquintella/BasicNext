# Architecture package — review status (2026-09-05)

> Canonical: `docs/architecture/review-status.md`  
> Honest status after stakeholder review. **Direction** can be approved while
> **contracts** remain incomplete. Do not treat “locked direction” as “closed
> contract.”

| Review point | Status | Notes |
| --- | --- | --- |
| Interpreter subordinate to specification | **Design met** | [conformance.md](conformance.md) requires spec-derived expectations + cross-backend |
| Eliminate `bn_ir → bn_frontend` | **Design met; packaging debt** | `bn_ir` uses `bn_types` (no `semantic::`); remaining debt is path-shim FE + thin-`bn` / GC-EXT — not semantic imports in IR model |
| Values, memory, ABI | **Partial** | Obligations listed; layouts / per-`bn_rt` symbol ownership **not** filled ([value-memory-abi.md](value-memory-abi.md), AQ-16) |
| Native stdlib binding (`bn_rt` shared, no stubs) | **Direction met; impl open** | [native-stdlib-binding.md](native-stdlib-binding.md); AQ-05 packaging; AQ-22 syntax |
| Requirements × support × policy | **Concept met; carrier open** | Dimensions locked; policy transport + defaults open (AQ-17) |
| Verifiable support matrix | **Partial** | Schema/shape locked; real inventory + evidence missing (AQ-08). **Architecture:** `validate_for` mandatory on every compile Backend before emit ([dfd-2/5.0](dfd/dfd-2/5.0 Compile IR.md)) — implementation still open |
| Snapshots, SourceId, LSP≡check | **Design met; impl pending** | [frontend-session.md](frontend-session.md); `bn_source` **must** be shared leaf (below) |
| Completion gates | **Partial** | GC-* + buckets strengthened; parity floor raised to **feature-scoped families** (below) |
| Well-formed IR guarantee | **MVP in code; contract incomplete** | CFG must-definition, explicit `Phi`, seed fixtures and initial `validate_for` landed; full op/type catalog and support inventory remain ([ir-contract.md](ir-contract.md), [to-close.md](to-close.md)) |

## Language baseline acceptance

[`../language/0.4/0.4.md`](../language/0.4/0.4.md) remains an **active 0.4 draft**: 0.4 amendments over an incorporated **0.3** baseline; G0 → accepted only after public contracts complete; G4 after executable conformance evidence. [`../../AGENTS.md`](../../AGENTS.md) correctly points agents at 0.4 as the *active planning* surface — that does **not** mean 0.4.md is “accepted final.”

## Four architecture corrections (this pass)

1. IR **definite assignment / CFG** validation (distinct from semantic “definitions by path”).
2. **`bn_source` shared leaf** under frontend, diag, and IR (closes AQ-04 direction).
3. Explicit **partial / not closed** labels on ir-contract, value-memory-abi, support-matrix.
4. **Parity**: claimed support ⇒ pertinent GC-PAR families (G3c tightened; AQ-19 updated).

## Code↔architecture audit (2026-09-06)

Bucket [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md) §C records divergences **D1–D10** and corrective actions **C-P0/C-P1/C-P2**. Major progress already in tree (crates, `validate_for`, Phi, Fluent, DataProvider, policy). Remaining P0: fake `.bn` stubs, mid-emit support inventory, physical `#[path]` elimination.

## Checklist to close

Executable checkbox list: **[to-close.md](to-close.md)**.

## See also

- [to-close.md](to-close.md)
- [completion-gates.md](completion-gates.md)
- [open-questions.md](open-questions.md)
- [README.md](README.md)
