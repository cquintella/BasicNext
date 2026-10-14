# Proposal: ordered module search path — MP1 (0.5.1)

**Status:** Proposed — toolchain / Frontend contract (**MP1**).
**Date:** 2026-09-14
**Target release:** **0.5.1** (orthogonal to T1 / H1 / DX1; **not** 0.5.2).
**Owner (tracker):** Tron until Carlos names implementer.
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-14):** Ship the architecture lock in [`docs/architecture/module-path.md`](../../docs/architecture/module-path.md) as a real ordered directory list (first hit wins), wired through Control → Frontend / `module_graph`, same for check / run / build.

**Related:** AQ-13 (default composition); T1 fixture T06 (relative imports / ordered paths); DFD 1.3 / 2.1 / process log.
**Non-goals:** `FROM` import syntax; dynamic download; inventing `--programs-dir` / `--plugins-dir` as 0.5.1 requirements; mapping logical names to arbitrary URLs; I1 qualified export (0.5.2).

Nothing here is normative until accepted into Control/Frontend and fixtures green; architecture direction was already locked 2026-09-05.

---

## Locked contract (Carlos 2026-09-14)

| Rule | Detail |
| --- | --- |
| Shape | **Ordered list of directories** |
| Resolution | Walk in order; **first hit wins** for each non-`HOST` import |
| CLI | `--module-path <dir>` **repeatable**; order = order on the command line |
| Config | Array `module-path = […]` (e.g. TOML) |
| Uniformity | **Same** resolution for `bn check`, `bn run`, `bn build` (and `bn eval` once T1 ships) |
| Wire | Real path in Frontend / `module_graph` — replace today’s entry-parent + heuristic `modules/bn` alone |
| Observability | `man` / `usage` + **process log** snapshot of the **effective** module-path |

---

## Defaults (AQ-13 — proposed minimum for 0.5.1)

When the user omits `--module-path` and config has no `module-path` array, effective path **at least**:

1. **Directory of the entry file**.
2. Discovered **stdlib** tree `modules/bn` (same discovery role as today).

Extra directories only via **`--module-path`** (repeatable) and/or config `module-path = […]`.
**Not in MP1 claim:** inventing or requiring `--programs-dir` / `--plugins-dir` (architecture may still note them as reserved/future without DoD).
**Merge:** CLI appends after config in documented order (CLI > config > defaults for *presence*; within CLI, left-to-right append). Exact merge table must be written in usage before implementation closes.

Optional short alias `-L <dir>` remains **out** of 0.5.1 unless Carlos expands.

---

## Semantics

1. Control (DFD **1.3**) builds the effective ordered list into **D_cfg**.
2. Analyze / Frontend (**2.1**, `module_graph::load`) resolves each `IMPORT` logical name by searching each directory for the corresponding `.bn` (existing layout rules: logical names, no `.bn` in source).
3. First filesystem hit binds; later directories are not searched for that import.
4. Shadow fixture: two directories both contain the same logical module → the earlier path’s file is loaded.
5. Stdlib still resolves when present on the effective path (default composition includes it).
6. Missing module → existing import-not-found diagnostics (structured under DX1 when migrated).
7. Process log records the effective list (absolute or normalized paths; safe to log).

---

## Acceptance fixtures (fixtures-first)

Evidence under `docs/superpowers/evidence/module-path-0.5.1/` (names final in bucket):

| ID | Intent |
| --- | --- |
| **MP01** | Two dirs on `--module-path`; import resolves from the intended first directory |
| **MP02** | **Shadow:** same logical module in dir A and dir B; A listed first → A wins; swap order → B wins |
| **MP03** | Defaults alone still resolve stdlib `modules/bn` (e.g. `IMPORT BNString AS S` or equivalent existing stdlib import) |
| **MP04** | Same effective path / same file chosen under `bn check`, `bn run`, and `bn build` for one fixture |
| **MP05** | Process log (or equivalent Control snapshot) contains the effective module-path list |
| **MP06** | Config `module-path = […]` without CLI matches documented merge; CLI append/override per table |

Minimum for claim green: **MP01, MP02, MP03** (two dirs, shadow, stdlib). MP04–MP06 required before tag unless Quorra accepts deferral text.

---

## Implementation sketch (not started)

- Wire Control flags + config parse → `Vec<PathBuf>` (or equivalent) on session / load request.
- Replace heuristic-only roots in `module_graph` with the supplied list (+ documented defaults when empty user list).
- Keep `HOST.*` on the Host production (unchanged).
- Update `usage` / man; book note if modules chapter mentions path.

---

## Open (narrow)

1. Exact CLI↔config merge when both specify paths (append vs replace) — **propose append: defaults, then config array, then each CLI `--module-path` in order**; lock in bucket decision register if Carlos wants the opposite.
2. Whether `-e` / eval uses cwd as “entry dir” for default #1 (align with T1 T06).

---

## History

- **2026-09-05** — Architecture direction locked (list of directories).
- **2026-09-14** — Carlos: ship as **MP1** in bucket **0.5.1**; confirm locks above; propose AQ-13 minimum default.
- **2026-09-14** — Carlos: simplify MP1 — no `--programs-dir` / `--plugins-dir` in claim; default = entry-file directory + stdlib `modules/bn`; extras only `--module-path` + config.
