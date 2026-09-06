# What remains to close (architecture contracts)

> Canonical: `docs/architecture/to-close.md`  
> **2026-09-05.** Direction can be approved while these stay open.  
> A contract is **closed** only when every checkbox in its section is done for the
> **claimed release slice** (shrink the claim if needed — do not fake closure).
>
> **Production bar:** provisional/incomplete solutions are **not** acceptable on
> user-facing paths. Prefer a smaller honest support claim over a stub gate.

Companion: [review-status.md](review-status.md) (design met vs partial).

---

## P0 — Must close before honest “well-formed IR” / G-SOFT GS1 quality

> **Updated 2026-09-06:** CFG must-definition + key `instruction_uses` holes +
> negative fixtures + `ValidatedModule` (W1) + AQ-20 `Phi` landed in tree.
> Remaining P0 is the full op/type catalog and complete support inventory.

### IR language `validate` (well-formed handoff)

- [x] **CFG definite assignment (MVP)** — reachable-CFG fixed-point / predecessor intersection in `src/ir/validate.rs` (not block-list order). Evidence: `tests/validated_ir.rs` (incl. one-branch diamond).
- [x] **`instruction_uses` for known holes** — `Input.prompt`, `Default.dynamic_dimensions` enumerated; negative tests cover undefined prompt/dimension.
- [x] **Full operand inventory** — audited every `Instruction` variant and field in `bn_ir::validate::instruction_uses`; all `ValueId` operands are enumerated, while symbol/class/name metadata is intentionally not a value operand. Keep the exhaustive match as the maintenance guard.
- [x] **Negative fixtures (seed)** — ≥3 classes in `tests/validated_ir.rs` (diamond / prompt / dynamic_dimensions).
- [x] **W1** — `ValidatedModule`; CLI paths use validated lower helpers (`ir.rs` / driver).
- [x] **AQ-20 decision** — explicit **`Phi`** locked in [ir-contract.md](ir-contract.md).
- [x] **AQ-20 implementation** — `Instruction::Phi` in `ir/model`, path-aware validation, interpreter selection and scalar LLVM emission.

### `validate` ≠ support (**production-complete** — no stub)

Production bar: do **not** leave `BUILD_LOWERING_UNAVAILABLE` / mid-emit
“unsupported” as the only story, and do **not** ship a `validate_for` that
always-oks or reuses `INVALID_IR_*`.

**Done only when all of the following hold:**

- [x] **`validate_for(module, Backend)`** is a real pre-emit gate on every
      `bn build` / compile profile path (native, wasm, …).
- [ ] **Stable support codes** shipped and used end-to-end:
      `TARGET_UNSUPPORTED_OP` | `TYPE` | `HOST` | `TARGET` (names exact or
      catalog-equivalent; documented in ir-contract + diag registry).
- [ ] **Inventory:** every current llvm `unsupported_instruction` /
      `BUILD_LOWERING_UNAVAILABLE` site either (a) is predicted by
      `validate_for` with the matching code, or (b) is deleted as dead.
- [ ] **Matrix slice for claimed compile support** is machine-readable (or
      in-tree table generating the same checks) with `reject_diag` + tests —
      no EXAMPLE fiction on claimed rows (AQ-08 for that slice).
- [ ] **Fixtures (mandatory):**
  - well-formed IR + unsupported-for-target → `validate` OK, `validate_for`
    fails with `TARGET_UNSUPPORTED_*` (not language-invalid);
  - ill-formed IR (e.g. diamond) → `validate` fails with IR code;
    `validate_for` not required to run;
  - at least one real HOST/op case taken from today’s llvm reject list.
- [ ] **CLI/LSP user-visible** text distinguishes support rejection from
      language/IR errors (Fluent may come in 0.4.5, but **codes** must already
      differ before prose upgrade).

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
- [ ] Stdlib native symbols (BNData/DataFrame subset) listed with ownership — [native-stdlib-binding.md](native-stdlib-binding.md)

### Native stdlib binding (direction locked; AQ-05 packaging / AQ-22 syntax)

- [ ] No empty `.bn` stubs for **claimed** stdlib APIs (production bar)
- [ ] DataFrame/I-O logic out of Executor special-cases into `bn_rt` (or one-way module)
- [ ] Interpret calls shared `bn_rt_*` for that subset (parity with llvm link)
- [ ] HostEnv **DataProvider** (or equivalent) — Executor does not hardcode CSV/table strings
- [ ] AQ-22 declaration syntax decided before EBNF freeze for native modules
- [ ] Dynamic `.so`/`.dylib`/`.dll` **not** required for 0.4.5; `--plugins-dir` stays toolchain-reserved

### Execution policy (AQ-17)

- [ ] Policy **carrier** into compiled artifacts (blob / env / `bn_rt` init — pick one)
- [ ] CLI **defaults** (default-deny vs permissive) decided
- [ ] `bn_rt` **re-check** at call boundary for claimed HOST subset (tests)

---

## P2 — Must close before hard-split “SM6 done” / GC-DEP

### IR independence (code)

- [X] Erase `semantic::{Type,SymbolId}` / `ModuleId` from public IR model — **done via `bn_types` / IR-owned ids** (2026-09-06); keep regression-guarded
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
  (See [`../../done/bucket-0.4.4.md`](../../done/bucket-0.4.4.md) — independent of **G-SOFT**.)

### Bucket 0.4.5 deliverables (after G-SOFT)

- [ ] Fluent / `bn_diag` (G4 track)
- [ ] Hard crate cuts with **GC-EXT** per move
- [ ] Optional `bnc` (G5)

---

## Already closed enough (do not re-open without cause)

| Item | State |
| --- | --- |
| Spec above interpret; conformance A+B | Design closed |
| Soft-prep **path** 0.4.4 before 0.4.5 | Path approved; G-SOFT deprecated — 0.4.5 may start |
| HOST three dimensions (concept) | Concept locked; carrier open (P1) |
| Stdlib → shared `bn_rt` + no empty stubs | Direction locked 2026-09-06; syntax AQ-22 |
| `bn_source` **below** frontend (direction) | Direction locked |
| Semantic “definitions by path” = name binding | Clarified; CFG is IR’s job |
| GC-* gate **names** and priority sequence | Locked; evidence still open |
| AGENTS.md points at 0.4 planning surface | Done; draft ≠ accepted |

---

## Doc sync note (2026-09-05)

Stakeholder review correctly flagged doc drift: treat CFG/`instruction_uses`/W1 seed as **done in code**; keep ir-contract **stub** for op catalog / Phi impl / types; keep support-matrix, value-memory-abi tables, AQ-17, AQ-18 open as below.

## Suggested close order

1. **P0** validate CFG + uses + W1/W2 (honest IR story; G-SOFT already closed)
2. **P1** matrix slice + ABI/policy carve-outs + claim-scoped parity  
3. **P2** `bn_source` cut + erase IR semantic types + session  
4. Hard split (0.4.5 §3) only with GC-EXT  
5. **P3** language G0/G4 and product 0.4.4 G4 on their own tracks  

## See also

- [review-status.md](review-status.md)
- [completion-gates.md](completion-gates.md)
- [ir-contract.md](ir-contract.md)
- [`../../done/bucket-0.4.4.md`](../../done/bucket-0.4.4.md)
- [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md)
