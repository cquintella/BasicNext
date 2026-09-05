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
| **AQ-04** | **`bn_source` placement** — standalone leaf crate vs module inside `bn_frontend` (spans/paths shared with `bn_diag`)? | **Open** — DAG diagram shows “`bn_source` / or inside frontend” | [target-architecture.md](target-architecture.md) (dependency DAG / ownership) |
| **AQ-05** | **Dataframe**: extract as its own crate vs keep as a **module** under runtime until deploy weight justifies a split? | **Open / deferred** — Value extract (`bn_value`) is mandatory first; dataframe crate premature per split rule | [target-architecture.md](target-architecture.md) (§ Breaking cycles; Dropped/deferred); XM2 |
| **AQ-06** | **When to split `bn-lsp` / `bn-dap` binaries** (or features) from the main `bn` deploy artifact? | **Recommended** eventually (XM11 / SM7); interim UX subcommands OK; exact release gate open | [target-architecture.md](target-architecture.md); [milestones-map.md](milestones-map.md) |
| **AQ-07** | **IR contract checklist** unfinished — module shape, op catalog, types, validation, versioning, LLVM subset, diagnostics boundary, backend invariants | **Stub** — expand before treating [ir-contract.md](ir-contract.md) as normative beyond locked bullets | [ir-contract.md](ir-contract.md); bucket 0.4.5 §2 / SM5 |
| **AQ-08** | **Interpret × LLVM support matrix** not filled from code — only EXAMPLE placeholders | **Stub** | [support-matrix.md](support-matrix.md); ir-contract LLVM subset bullet; XM0 |
| **AQ-09** | **Aliases for `bnc` subcommands** — older drafts used `check`/`run`/`build`; flag-shaped UX superseded them. Keep compatibility aliases? Document only? | **Flag UX locked**; subcommand draft superseded; whether to accept aliases later is **open** | [`../../audit/workpapers/09-synthesis/bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md); [`bnc-decisions.md`](../../audit/workpapers/09-synthesis/bnc-decisions.md) |
| **AQ-10** | Process-log **default verbosity** / filename scheme leftovers from critique (beyond MVP locks already taken) | **MVP locked** for companion log + `--log-level` + plugins reserved; broader critique items remain non-blocking | [`../../audit/workpapers/09-synthesis/pipeline-observability-critique.md`](../../audit/workpapers/09-synthesis/pipeline-observability-critique.md); target-architecture pipeline section |
| **AQ-11** | Optional **`bn_hir`** / alternate frontends | **Deferred P3** — revisit only if alt frontends appear | [target-architecture.md](target-architecture.md) (Dropped/deferred) |
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
