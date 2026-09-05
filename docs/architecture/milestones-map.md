# Milestone crosswalk — BasicNext architecture

> Canonical: `docs/architecture/milestones-map.md`  
> Prevents colliding **M0…** labels across docs.

Three numbering schemes existed. Use the **prefixes** below in new writing; keep historical “M*” inside each source file but link here.

| Prefix | Meaning | Canonical doc |
| --- | --- | --- |
| **SM** | Soft→hard **frontend/backend split** gates | [`../../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md) |
| **XM** | Crate-**extraction** / DAG migration roadmap | [`target-architecture.md`](target-architecture.md) § Migration order |
| **Bucket** | Release buckets (implementation checklists) | [`../../ongoing/bucket-0.4.4.md`](../../ongoing/bucket-0.4.4.md), [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md) |

---

## Soft→hard split (SM0–SM7)

| Id | Name | Typical bucket |
| --- | --- | --- |
| SM0 | Freeze boundary (docs) | prep / 0.4.4 |
| SM1 | Soft split in-tree + lint | 0.4.4 |
| SM2 | IR is the only bridge | 0.4.4 |
| SM3 | Break backend cycles | 0.4.4 → 0.4.5 leftover |
| SM4 | HOST spec out of frontend | 0.4.5 §1 |
| SM5 | Harden IR contract (minimum) | 0.4.5 §2 |
| SM6 | Hard split crates (workspace) | 0.4.5 §3 |
| SM7 | Optional: `bnc`, IDE binaries | 0.4.5 §5 (optional) |

`bucket-0.4.5.md` §0–§5 track **SM3 leftover → SM7** (plus Fluent diagnostics as a parallel track in that bucket, not an SM number).

---

## Crate extraction roadmap (XM0–XM11)

| Id | Name | Rough SM overlap |
| --- | --- | --- |
| XM0 | Document matrix + oracle (docs) | SM0 |
| XM1 | Normal `mod` layout for runtime | SM1-ish |
| XM2 | Extract `bn_value`; fix dataframe cycle | SM3 |
| XM3 | Handler trait inversion; fix http cycle | SM3 |
| XM4 | `FrontendSession`; wire CLI+LSP+DAP | SM1–SM2 |
| XM5 | Unified `Capabilities`; wasm console | SM3–SM4 area |
| XM6 | `bn_diag` + llvm Diagnostic return | 0.4.5 Fluent track |
| XM7 | `bn_host_spec`; catalogs out of semantic | SM4 |
| XM8 | Cut `bn_frontend`, `bn_ir` | SM5–SM6 |
| XM9 | Cut `bn_runtime` + host crates | SM6 |
| XM10 | Cut `bn_llvm` optional; validate_for | SM6 |
| XM11 | `bn-lsp` / `bn-dap` binaries or features | SM7 |

XM and SM are **parallel views** (extract vs soft/hard). Do not assume `Mn` in one file equals `Mn` in another.

---

## Release buckets

| Bucket | Intent |
| --- | --- |
| **0.4.4** | Pre-refactor: SM0–SM2, start SM3 — bug-fix / soft prep |
| **0.4.5** | Hard split SM3leftover–SM6 + **Fluent / expressive diagnostics**; SM7/`bnc` optional |

---

## Related product locks (not milestone numbers)

- Language posture minimalist — `target-architecture.md`
- Normative syntax — [`../language/0.4/0.4.ebnf`](../language/0.4/0.4.ebnf)
- `bnc` UX + log MVP — `bnc-decisions.md` / DFDs
