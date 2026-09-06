# Open architecture questions

> Canonical: `docs/architecture/open-questions.md`  
> Status: living register from architecture review — **2026-09-05**  
> Each row is a decision still open or only partially locked. Prefer updating
> status here when stakeholders decide; do not bury new forks only in chat.

This register does **not** replace milestone plans ([milestones-map.md](milestones-map.md))
or bucket checklists. It lists **decisions** that architecture docs currently
treat as unresolved, optional, or wording-sensitive.

---

## Register

| Id | Question | Status | Where discussed |
| --- | --- | --- | --- |
| **AQ-01** | Should IDE / LSP / DAP **subscribe to pipeline events** (same event bus as the companion process log) in a near release, or remain strictly **future**? | **Future** — possibility left open; **not MVP** | [target-architecture.md](target-architecture.md) (pipeline management); [`../../audit/workpapers/09-synthesis/bnc-decisions.md`](../../audit/workpapers/09-synthesis/bnc-decisions.md); [README.md](README.md) locked-vs-proposed table; [sequences.md](sequences.md) |
| **AQ-02** | Ship a separate **`bnc` binary** in bucket **0.4.5**, or land companion log + flags on **`bn` first** and keep `bnc` optional? | **Optional / proposed** for 0.4.5 (SM7); role + CLI UX locked | [README.md](README.md); [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md); [`../../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md) (SM7); [`bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md) |
| **AQ-03** | **Fluent** catalogs for `bn_diag`: stakeholder wording — “chosen for 0.4.5” vs fully locked normative language for all diagnostics paths? | **Chosen for 0.4.5** (proposal); wire in **XM6**; exact lock wording may still need a one-line stakeholder confirmation in README | [`../../todo/proposals/expressive-diagnostics.md`](../../todo/proposals/expressive-diagnostics.md); [README.md](README.md); [milestones-map.md](milestones-map.md) |
| **AQ-04** | **`bn_source` placement** | **Direction locked 2026-09-05:** shared **leaf below** frontend (`bn_diag` + `bn_ir` + `bn_frontend` depend on it). Exact crate path/name trivial. | [frontend-session.md](frontend-session.md); [target-architecture.md](target-architecture.md) |
| **AQ-18** | Exact `FrontendSession` API (method names, debounce, dependency-graph invalidation algorithm, revision representation) | **Capabilities locked**; API details **open** | [frontend-session.md](frontend-session.md); XM4 |
| **AQ-20** | IR merge/`φ` form | **Locked 2026-09-05:** explicit **`Phi`** instruction `(preds → new ValueId)`; until implemented, CFG validate rejects multi-def joins | [ir-contract.md](ir-contract.md) § Merge / φ form |
| **AQ-19** | Staging of GC-PAR inside 0.4.5 | **Updated 2026-09-05:** G3c requires **pertinent families per claimed support row**, not one global extra family. Staging by *shrinking the claimed matrix* is OK; staging by under-testing announced support is **not**. | [completion-gates.md](completion-gates.md); bucket G3c |
| **AQ-05** | **Dataframe**: extract as its own crate vs keep as a **module** under runtime until deploy weight justifies a split? | **Open / deferred** — Value extract (`bn_value`) is mandatory first; dataframe crate premature per split rule | [target-architecture.md](target-architecture.md) (§ Breaking cycles; Dropped/deferred); XM2 |
| **AQ-06** | **When to split `bn-lsp` / `bn-dap` binaries** (or features) from the main `bn` deploy artifact? | **Recommended** eventually (XM11 / SM7); interim UX subcommands OK; exact release gate open | [target-architecture.md](target-architecture.md); [milestones-map.md](milestones-map.md) |
| **AQ-07** | **IR contract checklist** unfinished — module shape, op catalog, types, validation, versioning, LLVM subset, diagnostics boundary, backend invariants | **Stub** — expand before treating [ir-contract.md](ir-contract.md) as normative beyond locked bullets | [ir-contract.md](ir-contract.md); bucket 0.4.5 §2 / SM5 |
| **AQ-08** | Fill structured support-matrix catalog from code; choose on-disk format (TOML/YAML/JSON); wire coverage + `reject_diag` family names | **Contract shape locked**; **data/format open** | [support-matrix.md](support-matrix.md); [conformance.md](conformance.md); `tests/test_compiler_parity.py` |
| **AQ-09** | **Aliases for `bnc` subcommands** — older drafts used `check`/`run`/`build`; flag-shaped UX superseded them. Keep compatibility aliases? Document only? | **Flag UX locked**; subcommand draft superseded; whether to accept aliases later is **open** | [`../../audit/workpapers/09-synthesis/bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md); [`bnc-decisions.md`](../../audit/workpapers/09-synthesis/bnc-decisions.md) |
| **AQ-10** | Process-log **default verbosity** / filename scheme leftovers from critique (beyond MVP locks already taken) | **MVP locked** for companion log + `--log-level` + plugins reserved; broader critique items remain non-blocking | [`../../audit/workpapers/09-synthesis/pipeline-observability-critique.md`](../../audit/workpapers/09-synthesis/pipeline-observability-critique.md); target-architecture pipeline section |
| **AQ-11** | Optional **`bn_hir`** / alternate frontends | **Deferred P3** — revisit only if alt frontends appear | [target-architecture.md](target-architecture.md) (Dropped/deferred) |
| **AQ-17** | Concrete policy carrier for compiled artifacts (embedded blob vs env vs `bn_rt` init API) and default-deny vs permissive CLI defaults | **Dimensions locked** in [host-traits.md](host-traits.md); carrier/default **open** (XM5) | [host-traits.md](host-traits.md); threat model |
| **AQ-16** | Concrete layout tables and per-`bn_rt` symbol ownership docs (fill from code); which helpers interpret must call via `bn_rt` next | **Categories locked** in [value-memory-abi.md](value-memory-abi.md); catalog fill **open** | [value-memory-abi.md](value-memory-abi.md); `crates/bn_rt` |
| **AQ-15** | Migration order to erase `semantic::{Type,SymbolId}` / `ModuleId` from `ir/model` when cutting `bn_ir` (replace with IR-owned ids) | **Direction locked** (no `bn_ir → FE`); sequencing inside XM8 **open** | [target-architecture.md](target-architecture.md); `src/ir/model.rs` |
| **AQ-14** | Fine-grained diagnostic codes / which 0.4 rules map 1:1 into each semantic-analysis § | **Categories locked** in [semantic-analysis.md](semantic-analysis.md); per-rule catalog follows `0.4.md` + diagnostics proposal | [semantic-analysis.md](semantic-analysis.md); [`0.4.md`](../language/0.4/0.4.md) |
| **AQ-13** | Exact **default composition** of module-path when CLI omits `--module-path` (where stdlib `modules/bn` sits in the order; whether `-L` alias ships) | **Direction locked** (path is a **list**); default composition / short alias **open** | [module-path.md](module-path.md); [`bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md) |
| **AQ-12** | HOST **bound-function** syntax / GPU–DOM profiles (product capability model) | **Exploratory proposal** — not 0.4 toolchain gate | [`../../todo/proposals/host-capabilities.md`](../../todo/proposals/host-capabilities.md); [host-traits.md](host-traits.md) |

---

## How to close a row

1. Record the stakeholder decision in the “Where discussed” primary doc (or a
   dated note linked from [README.md](README.md)).
2. Set **Status** here to **Locked**, **Rejected**, or **Deferred** with the
   date.
3. If the decision changes DFDs or the IR contract, update those files in the
   same change set — do not leave sequences/DFDs contradicting this register.

## See also

- [README.md](README.md) — locked vs proposed short table
- [glossary.md](glossary.md)
- [nfr-security.md](nfr-security.md) — security sources of truth (not open unless noted)
