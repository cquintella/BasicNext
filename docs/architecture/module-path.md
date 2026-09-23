# Module search path (to-be)

> Canonical: `docs/architecture/module-path.md`  
> Status: **architecture direction locked 2026-09-05**; **0.5.1 claim MP1 accepted** (Quorra re-gate 2026-09-14) — see [`todo/proposals/module-path-0.5.1.md`](../../todo/proposals/module-path-0.5.1.md) and `ongoing/bucket-0.5.1.md`. CLI/config paths are carried through Control/Frontend into `module_graph`; effective ordered roots and first-hit behavior are covered by unit/integration evidence.

## Problem

Programs `IMPORT` other `.bn` modules (user libraries and standard library). The toolchain must know **where to look** when resolving those imports for **check / interpret / compile** (same Frontend path). A single project root is not enough when libraries live in several trees (workspace packages, shared `modules/`, installed stdlib, CI overlays).

## Decision

Expose an **ordered module search path**: a **list of directories**. The loader walks the list in order and uses the **first** hit for each non-`HOST` import.

This is independent of **`--plugins-dir`** (reserved; toolchain plugins, not BN source modules).

### CLI / config

| Surface | Shape | Notes |
| --- | --- | --- |
| `--module-path <dir>` | **repeatable** | Each occurrence appends one directory (order = CLI order). |
| Config `module-path` | array of paths | e.g. `module-path = [".", "vendor/bn", "/opt/bn/modules"]` |
| Precedence | CLI appends/overrides per documented merge | CLI > config > defaults (same as other Control settings). |

Optional short alias later: `-L <dir>` (linker-style); not required for MVP naming.

### Relation to `--programs-dir`

| Flag | Role |
| --- | --- |
| **`--programs-dir`** | Project / programs **root** for resolving the **entry** `.bn` and default layout heuristics. |
| **`--module-path`** | **Ordered list** of directories searched for **imported** `.bn` modules (link/resolve). |

Defaults (illustrative): if `--module-path` is omitted, the effective list still includes at least:

1. Directory of the entry file (**MP1 default**).
2. The entry `modules/` compatibility root (when present).
3. The resolved **standard-library** tree (`modules/bn` — same role as today’s loader).
4. Any paths from config / CLI `--module-path` when present.

`--programs-dir` / `--plugins-dir`: reserved/future in architecture diagrams; **not** required to ship MP1.

Exact default composition was open as **AQ-13**; **MP1 (0.5.1)** locks the ordered defaults: entry directory, compatibility `entry/modules` root, and discovered stdlib `modules/bn`; extras only via `--module-path` / config. `--programs-dir` / `--plugins-dir` remain **reserved/future** architecture notes — **not** MP1 DoD (Carlos 2026-09-14). The **architecture requirement** remains: **the search path is a list**, not a single directory, and it is visible in Control config + process log.

### Who consumes it

| DFD | Use |
| --- | --- |
| **1.3 Resolve directories** | Builds the effective ordered list into **D_cfg** (with programs-dir / plugins-dir reserved). |
| **2.1 Load entry and modules** | Uses that list when resolving `IMPORT` paths (loop A12). |
| **6.0 Process log** | Records the effective module-path snapshot (safe to log). |

Compile (`-c`) and interpret share the **same** resolution rules so link and run cannot disagree on which `.bn` file was chosen.

### Non-goals

- Mapping logical import names to arbitrary filesystem URLs in MVP (keep logical module names as in `0.4.md`).
- Dynamic download of modules.
- Plugin ABI via `--plugins-dir`.

### Provenance and `BN_HOME` (0.5.1a, D-F8-03)

Every effective root carries a typed `RootProvenance` (`EntryDir`, `BnHome`,
`EntryAncestor`, `Cwd`, `ExeInstall`, `StdlibDefault`, `Config`, `CliFlag`),
and every resolved module records the root that won. The process log
(`--log-level debug`) prints both, so an ancestor `modules/bn` can never
hijack resolution silently.

`BN_HOME`, when set, names the standard library explicitly
(`$BN_HOME/share/bn/modules/bn`, else `$BN_HOME/modules/bn`) and **replaces**
the ancestor/cwd/executable search. It is fail-closed: a `BN_HOME` without a
stdlib is still used as the stdlib root, so standard imports fail at that path
instead of falling through. Ordering of the effective list is unchanged:
entry dir, `entry/modules`, stdlib, config extras, CLI extras.

## As-is today

`module_graph::load_with_overlays_and_paths` builds the ordered list above
(MP1, 0.5.1) with provenance and `BN_HOME` (0.5.1a). `--programs-dir` /
`--plugins-dir` remain reserved.

## See also

- [dfd/dfd-2/1.0 Control.md](dfd/dfd-2/1.0 Control.md) (1.3)
- [dfd/dfd-2/2.0 Analyze Sources.md](dfd/dfd-2/2.0 Analyze Sources.md) (2.1)
- [`../../audit/workpapers/09-synthesis/bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md)
- Language modules layout: [`../../language/0.4/0.4.md`](../../language/0.4/0.4.md) (`modules/` vs `modules/bn/`)
