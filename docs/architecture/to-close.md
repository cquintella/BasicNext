# What remains to close (architecture contracts)

> Canonical: `docs/architecture/to-close.md`  
> **2026-09-05.** Direction can be approved while these stay open.  
> A contract is **closed** only when every checkbox in its section is done for the
> **claimed release slice** (shrink the claim if needed — do not fake closure).

Companion: [review-status.md](review-status.md) (design met vs partial).

---

## P0 — Must close before honest “well-formed IR” / G-SOFT GS1 quality

> **2026-09-05:** Architecture/API decisions for Phi + `validate_for` are locked. **Implementation deferred** (stakeholder: code later). Keep checkboxes open until code lands.


### IR language `validate` (well-formed handoff)

- [ ] **CFG definite assignment** — every `ValueId` use defined on **all executable paths** (joins, loops, diamonds); not block-list `HashSet` order ([ir-contract.md](ir-contract.md) § Definite assignment)
- [ ] **Complete `instruction_uses`** — include at least `Input.prompt`, `Default.dynamic_dimensions`, and audit all other operand fields (`src/ir.rs` / `validate.rs`)
- [ ] **Negative fixtures** — ≥3 CFG/use-enumeration failure classes with stable **language** codes (W2 / GC-IR)
- [ ] **W1** — `run`/`build` only consume post-`validate` IR (assert/`ValidatedModule`)
- [x] **AQ-20** — **explicit `Phi`** locked in ir-contract; implement in code (still open)

### `validate` ≠ support

- [ ] **`validate_for` / support check** path distinct from language validate (stub OK in 0.4.4; real codes in 0.4.5 G2b)
- [ ] Unsupported-llvm fixture fails **support** diagnostic family, not “invalid IR”

---

## P1 — Must close before announcing a compile support subset

### Support matrix (AQ-08)

- [ ] Choose on-disk format (TOML/YAML/JSON)
- [ ] Inventory real ops×types×targets from `ir/model` + llvm emission
- [ ] Delete EXAMPLE fiction from any **claimed** rows
- [ ] Each claimed row cites **tests** + `reject_diag`
- [ ] Coverage gap report (ops without tests)

### Parity / GC-PAR (AQ-19 / G3c)

- [ ] For **each claimed** matrix row, pertinent families have evidence:
  - [ ] Numeric boundaries (where numeric)
  - [ ] Errors / traps / toolchain failures (where language defines)
  - [ ] Observable effects (where HOST/I/O)
  - [ ] Objects (aliasing / `DELETE` / dispatch) where objects claimed
  - [ ] Opt stability where opts claimed to preserve semantics
- [ ] Spec-derived fixtures (conformance **A**), not only interpret↔compile **B**

### Value / memory / ABI (AQ-16)

- [ ] Layout/alignment tables for ABI-visible types in the slice
- [ ] Per-`bn_rt` symbol ownership (borrow/transfer/free) for every extern used by lowering
- [ ] Numeric lowering rules written so LLVM `nsw`/poison ≠ BN `Error` with fixtures
- [ ] Carve-outs explicit for anything not in the slice

### Execution policy (AQ-17)

- [ ] Policy **carrier** into compiled artifacts (blob / env / `bn_rt` init — pick one)
- [ ] CLI **defaults** (default-deny vs permissive) decided
- [ ] `bn_rt` **re-check** at call boundary for claimed HOST subset (tests)

---

## P2 — Must close before hard-split “SM6 done” / GC-DEP

### IR independence (code)

- [ ] Erase `semantic::{Type,SymbolId}` / `ModuleId` from public IR model (AQ-15 sequencing)
- [ ] No `bn_ir → bn_frontend` in the crate graph
- [ ] Allowlisted BE peeks at AST/semantic removed or empty

### `bn_source` leaf (AQ-04 direction locked; packaging left)

- [ ] Cut/place `bn_source` so `bn_diag` + `bn_ir` + `bn_frontend` depend on it
- [ ] `Span` carries `SourceId`; lowering preserves it on IR debug locs
- [ ] DAG diagram in [target-architecture.md](target-architecture.md) shows the leaf explicitly in Mermaid (ownership table already lists it)

### FrontendSession (AQ-18)

- [ ] Snapshots for unsaved buffers + revisions
- [ ] Dependent invalidation + cancel
- [ ] Revision-scoped `publishDiagnostics`
- [ ] Baseline LSP Problems ≡ `--check` stages (same validate)
- [ ] Exact API names/debounce (can ship incrementally after capabilities)

---

## P3 — Product / language acceptance (parallel; not architecture-direction blockers)

### Language `0.4.md`

- [ ] Resolve Phase 0 API/provider/limit/dependency blockers called out in the draft status
- [ ] **G0** — public contracts complete → status **accepted** (today: **active draft** over 0.3)
- [ ] **G4** — executable conformance evidence for the language release

### Bucket 0.4.4 product G4 (bug-fix)

- [ ] G0 Clippy/CI matrix
- [ ] G1 HOST.Net native differential + handles
- [ ] G2 HTTPS / BNWeb stubs
- [ ] G3 DAP/LSP/Wasm advertisements
- [ ] G4 evidence + version bump  
  (See [`../../ongoing/bucket-0.4.4.md`](../../ongoing/bucket-0.4.4.md) — independent of **G-SOFT**.)

### Bucket 0.4.5 deliverables (after G-SOFT)

- [ ] Fluent / `bn_diag` (G4 track)
- [ ] Hard crate cuts with **GC-EXT** per move
- [ ] Optional `bnc` (G5)

---

## Already closed enough (do not re-open without cause)

| Item | State |
| --- | --- |
| Spec above interpret; conformance A+B | Design closed |
| Soft-prep **path** 0.4.4 before 0.4.5 | Path approved; execute G-SOFT |
| HOST three dimensions (concept) | Concept locked; carrier open (P1) |
| `bn_source` **below** frontend (direction) | Direction locked |
| Semantic “definitions by path” = name binding | Clarified; CFG is IR’s job |
| GC-* gate **names** and priority sequence | Locked; evidence still open |
| AGENTS.md points at 0.4 planning surface | Done; draft ≠ accepted |

---

## Suggested close order

1. **P0** validate CFG + uses + W1/W2 (unblocks honest IR story; feeds G-SOFT)  
2. **P1** matrix slice + ABI/policy carve-outs + claim-scoped parity  
3. **P2** `bn_source` cut + erase IR semantic types + session  
4. Hard split (0.4.5 §3) only with GC-EXT  
5. **P3** language G0/G4 and product 0.4.4 G4 on their own tracks  

## See also

- [review-status.md](review-status.md)
- [completion-gates.md](completion-gates.md)
- [ir-contract.md](ir-contract.md)
- [`../../ongoing/bucket-0.4.4.md`](../../ongoing/bucket-0.4.4.md)
- [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md)
