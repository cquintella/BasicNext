# BN IR contract (to-be) — stub

> Canonical: `docs/architecture/ir-contract.md`  
> **Status:** stub / draft **2026-09-05** — **not complete**. Expand before treating as normative beyond the bullets below.

## Purpose

Define the **single Intermediate Representation (BN IR)** that sits between the Frontend (Analyze Sources → Lower and Validate IR) and the Backend legs **interpret** and **compile**. One IR for the job; both backends consume it from store **D2**.

## Normative posture (locked direction)

- **Interpret is the semantic oracle.** Running a program means executing validated BN IR under HostEnv — not LLVM `lli`, and not a private AST interpreter.
- **Compile lowers the same IR.** Compile reads the same validated BN IR and lowers BN IR → LLVM IR for the external clang/ld/opt toolchain. It must not invent a second meaning from AST alone.
- Frontend produces AST + symbols; process **3.0** lowers and validates into BN IR. Backends do not re-lower from AST.

## Pointers

| Topic | Where |
| --- | --- |
| Lower / validate (DFD-2 3.0) | [dfd/dfd-2/3.0 Lower and Validate IR.md](dfd/dfd-2/3.0 Lower and Validate IR.md) |
| Interpret (DFD-2 4.0) | [dfd/dfd-2/4.0 Interpret IR.md](dfd/dfd-2/4.0 Interpret IR.md) |
| Compile (DFD-2 5.0) | [dfd/dfd-2/5.0 Compile IR.md](dfd/dfd-2/5.0 Compile IR.md) |
| Language static semantics / runtime (0.4) | [../language/0.4/0.4.md](../language/0.4/0.4.md) |
| Data dictionary (D2, L/I/G flows) | [dfd/data-dictionary.md](dfd/data-dictionary.md) |
| Split milestones (**SM5** / IR contract minimum — see [milestones-map.md](milestones-map.md)) | [`../../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md) |
| Bucket 0.4.5 §2 / SM5 activities | [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md) (SECTION 2 — IR contract minimum) |
| Related proposals (if any) | [`../../todo/proposals/`](../../todo/proposals/) — e.g. [llvm-ir-optimization.md](../../todo/proposals/llvm-ir-optimization.md); no dedicated IR-shape proposal yet |

## Checklist — what the full contract must eventually define

- [ ] **Module / program shape** — units, linkage of modules, entry, how imports appear in IR.
- [ ] **Ops / instruction set** — complete op catalog (kinds, operands, side-effect notes) aligned with `ir/model` (and crate `bn_ir` after split).
- [ ] **Types** — IR type system vs language types; lowering rules; HOST value boundaries.
- [ ] **Validation rules** — structural and light semantic checks performed by 3.2; error codes into **D3**.
- [ ] **Versioning** — IR format / schema version; compatibility expectations across toolchain releases.
- [ ] **LLVM subset matrix** — which BN IR ops are supported for compile vs interpret-only; see stub [support-matrix.md](support-matrix.md) (fill from code; EXAMPLE rows are not coverage). Unsupported-for-llvm should fail validate with a stable code (bucket 0.4.5 G2).
- [ ] **Diagnostics on FE→IR path** — lower/validate use `Diagnostic` (no free-form `String` on the contract boundary).
- [ ] **Invariants for backends** — interpret and compile consumption rules; no AST fork; HostEnv vs `bn_rt` boundaries.

## Non-goals for this stub

- Does not replace the language EBNF or `0.4.md`.
- Does not specify Fluent catalogs or CLI UX (`bnc` options).
- Does not freeze crate layout; it only names the contract surface backends must share.

## See also

- [Architecture README](README.md)
- [target-architecture.md](target-architecture.md)
- [milestones-map.md](milestones-map.md)
- [support-matrix.md](support-matrix.md) — interpret × llvm matrix stub
