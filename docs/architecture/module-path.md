# Module search path (to-be)

> Canonical: `docs/architecture/module-path.md`  
> Status: **architecture direction locked 2026-09-05** (list of directories). Implementation still follows today’s single-root + `modules/bn` heuristic until Control/Frontend wire this.

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

1. Directory of the entry file (or `--programs-dir` when that is the project root convention).
2. The resolved **standard-library** tree (`modules/bn` — same role as today’s loader).
3. Any paths from config when present.

Exact default composition is an implementation detail; the **architecture requirement** is: **the search path is a list**, not a single directory, and it is visible in Control config + process log.

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

## As-is today

`module_graph::load` uses the entry’s parent directory plus a discovered `modules/bn` standard directory — **not** yet a user-supplied list. This document is the to-be contract for Control + Frontend.

## See also

- [dfd/dfd-2/1.0 Control.md](dfd/dfd-2/1.0 Control.md) (1.3)
- [dfd/dfd-2/2.0 Analyze Sources.md](dfd/dfd-2/2.0 Analyze Sources.md) (2.1)
- [`../../audit/workpapers/09-synthesis/bnc-options.md`](../../audit/workpapers/09-synthesis/bnc-options.md)
- Language modules layout: [`../language/0.4/0.4.md`](../language/0.4/0.4.md) (`modules/` vs `modules/bn/`)
