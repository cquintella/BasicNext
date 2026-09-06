# Architecture package — review status (2026-09-05)

> Canonical: `docs/architecture/review-status.md`  
> Honest status after stakeholder review. **Direction** can be approved while
> **contracts** remain incomplete. Do not treat “locked direction” as “closed
> contract.”

| Review point | Status | Notes |
| --- | --- | --- |
| Interpreter subordinate to specification | **Design met** | [conformance.md](conformance.md) requires spec-derived expectations + cross-backend |
| Eliminate `bn_ir → bn_frontend` | **Design met; code debt** | DAG/ownership corrected; `src/ir/model.rs` still imports semantic types |
| Values, memory, ABI | **Partial** | Obligations listed; layouts / per-`bn_rt` symbol ownership **not** filled ([value-memory-abi.md](value-memory-abi.md), AQ-16) |
| Requirements × support × policy | **Concept met; carrier open** | Dimensions locked; policy transport + defaults open (AQ-17) |
| Verifiable support matrix | **Partial** | Schema/shape locked; real inventory + evidence missing (AQ-08) |
| Snapshots, SourceId, LSP≡check | **Design met; impl pending** | [frontend-session.md](frontend-session.md); `bn_source` **must** be shared leaf (below) |
| Completion gates | **Partial** | GC-* + buckets strengthened; parity floor raised to **feature-scoped families** (below) |
| Well-formed IR guarantee | **Insufficient until CFG rules land** | Definite assignment on **all executable paths** is an **IR `validate`** obligation — not FE name resolution ([ir-contract.md](ir-contract.md) § Definite assignment) |

## Language baseline acceptance

[`../language/0.4/0.4.md`](../language/0.4/0.4.md) remains an **active 0.4 draft**: 0.4 amendments over an incorporated **0.3** baseline; G0 → accepted only after public contracts complete; G4 after executable conformance evidence. [`../../AGENTS.md`](../../AGENTS.md) correctly points agents at 0.4 as the *active planning* surface — that does **not** mean 0.4.md is “accepted final.”

## Four architecture corrections (this pass)

1. IR **definite assignment / CFG** validation (distinct from semantic “definitions by path”).
2. **`bn_source` shared leaf** under frontend, diag, and IR (closes AQ-04 direction).
3. Explicit **partial / not closed** labels on ir-contract, value-memory-abi, support-matrix.
4. **Parity**: claimed support ⇒ pertinent GC-PAR families (G3c tightened; AQ-19 updated).

## Checklist to close

Executable checkbox list: **[to-close.md](to-close.md)**.

## See also

- [to-close.md](to-close.md)
- [completion-gates.md](completion-gates.md)
- [open-questions.md](open-questions.md)
- [README.md](README.md)
