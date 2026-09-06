# BN IR contract (to-be) — stub

> Canonical: `docs/architecture/ir-contract.md`  
> **Status:** **stub / incomplete** (2026-09-05). Direction and handoff **W1–W5** are locked; **op catalog, types, and full validation rules are not a closed contract.** Fill the **next release slice** with concrete rules + evidence before claiming closed. See [review-status.md](review-status.md).

## Purpose

Define the **single Intermediate Representation (BN IR)** that sits between the Frontend (Analyze Sources → Lower and Validate IR) and the Backend legs **interpret** and **compile**. One IR for the job; both backends consume it from store **D2**.

## Normative posture (locked direction)

### `validate` vs target support (locked 2026-09-05)

- **`validate`**: “Is this IR valid **as BN**?” Failure ⇒ language/IR defect.
- **Support check / `validate_for(target)`**: “Does **this target** implement this IR under the [support-matrix.md](support-matrix.md)?” Failure ⇒ valid program, unsupported here.
- Do not collapse the two. Matrix rows must name `reject_diag` + `tests`.

**API shape (to implement):**

```rust
// Language IR — CFG definite assignment, complete uses, structure
pub fn validate(module: &Module) -> Result<(), Diagnostic>

// Backend handoff proof object (0.4.4 S1.1)
pub fn validate_module(module: Module) -> Result<ValidatedModule, Diagnostic>

pub enum Backend { Interpreter, LlvmNative, Wasm32 /* … */ }

// Target support — NEVER called as a substitute for validate
pub fn validate_for(module: &Module, backend: Backend) -> Result<(), Diagnostic>
// On failure: TARGET_UNSUPPORTED_* (or catalog id), not INVALID_IR_*
```

`build` / compile profiles: `validate` then `validate_for(Llvm*)`. `run` / interpret: `validate` then optional `validate_for(Interpreter)` if the matrix distinguishes.


### Crate independence (locked 2026-09-05)

| Component | Owns |
| --- | --- |
| Frontend | AST, resolution, semantic analysis, **lowering → BN IR** |
| `bn_ir` | Own types, instructions, identities, **validation** |
| Interpreter | Execute validated IR |
| LLVM backend | Validated IR → target |

No `bn_ir → bn_frontend` edge. No new crate/HIR required to satisfy this — move lowering under frontend when splitting.


- **Interpret is the executable reference** (subordinate to the language specification). Running a program means executing validated BN IR under HostEnv — not LLVM `lli`, and not a private AST interpreter. Compiler equivalence is judged against the **spec** (and the support matrix), not against “whatever the interpreter did when buggy.” See [conformance.md](conformance.md).
- **Compile lowers the same IR.** Compile reads the same validated BN IR and lowers BN IR → LLVM IR for the external clang/ld/opt toolchain. It must not invent a second meaning from AST alone.
- Frontend produces AST + symbols; **2.5** must satisfy [semantic-analysis.md](semantic-analysis.md). **Lowering** (AST+semantic → BN IR) is a **Frontend** responsibility; **`bn_ir` must not depend on `bn_frontend`/semantic**. Process **3.0** in the DFD is the logical lower+validate stage: lower runs in the frontend crate, **validate** is the IR crate’s job. Backends consume validated IR only and do not re-lower from AST.
- **As-is debt:** `src/ir/model.rs` still imports `semantic::{SymbolId, Type}` and `module_graph::ModuleId` — forbidden in the to-be `bn_ir` public model.

## Pointers

| Topic | Where |
| --- | --- |
| Lower / validate (DFD-2 3.0) | [dfd/dfd-2/3.0 Lower and Validate IR.md](dfd/dfd-2/3.0 Lower and Validate IR.md) |
| Interpret (DFD-2 4.0) | [dfd/dfd-2/4.0 Interpret IR.md](dfd/dfd-2/4.0 Interpret IR.md) |
| Compile (DFD-2 5.0) | [dfd/dfd-2/5.0 Compile IR.md](dfd/dfd-2/5.0 Compile IR.md) |
| Language static semantics / runtime (0.4) | [../language/0.4/0.4.md](../language/0.4/0.4.md) |
| **Semantic analysis contract (2.5)** | [semantic-analysis.md](semantic-analysis.md) — required *before* advertising validated IR |
| Data dictionary (D2, L/I/G flows) | [dfd/data-dictionary.md](dfd/data-dictionary.md) |
| Split milestones (**SM5** / IR contract minimum — see [milestones-map.md](milestones-map.md)) | [`../../audit/workpapers/09-synthesis/fe-be-split-milestones.md`](../../audit/workpapers/09-synthesis/fe-be-split-milestones.md) |
| Bucket 0.4.5 §2 / SM5 activities | [`../../ongoing/bucket-0.4.5.md`](../../ongoing/bucket-0.4.5.md) (SECTION 2 — IR contract minimum) |
| Related proposals (if any) | [`../../todo/proposals/`](../../todo/proposals/) — e.g. [llvm-ir-optimization.md](../../todo/proposals/llvm-ir-optimization.md); no dedicated IR-shape proposal yet |



## Definite assignment on executable paths (IR `validate` — required)

**Problem misfiled earlier:** “definitions available by path” in
[semantic-analysis.md](semantic-analysis.md) is **name/module binding**. The
stakeholder requirement for well-formed IR is different:

> Every **value** used by an instruction or terminator must be **defined on all
> executable control-flow paths** that reach that use (joins, loops, multiple
> definitions, every operand).

Frontend name resolution **cannot** certify a defective IR after lowering.
That check belongs in **`bn_ir` `validate`** (language IR validity — **W2** / **GC-IR**).

### Normative rules (contract; implementation must match)

1. Build the function CFG from blocks + terminators (`Jump` / `Branch` / `Return` / `Stop`).
2. For each use of a `ValueId`, every path from the entry (or from dominating defs
   per SSA/`φ` rules the IR adopts) must define that value before the use.
3. At **joins**, a use is valid only if the value is defined on **all** incoming
   paths (or the IR provides an explicit merge/`φ` the validator understands).
4. **Loops:** definitions inside loops do not automatically dominate uses outside
   without a valid loop-carried story; uses must not see “maybe defined.”
5. **All operands** count — including nested fields of instructions (see as-is gaps).
6. Failure ⇒ **language** IR diagnostic (stable code), not a target-support code.

### As-is debt (`src/ir/validate.rs`)

Current validate accumulates a `HashSet` of definitions by **block list order**,
not by CFG dominance / path merge. That misses branch/join/loop defects.

The 0.4.4 S1.2 slice now performs reachable-CFG must-definition analysis and
enumerates `Input.prompt` and `Default.dynamic_dimensions`. Its negative and
positive fixtures live in `tests/validated_ir.rs`; the full contract remains
open until the remaining operand inventory and explicit `Phi` support are
closed.

Also, `instruction_uses` is incomplete — known holes (must be fixed as part of
GC-IR / W2, not waved through):

| Instruction field | Issue |
| --- | --- |
| `Input.prompt` | Present on the instruction; **omitted** from `instruction_uses` |
| `Default.dynamic_dimensions` | `Vec<ValueId>` on the instruction; **omitted** from `instruction_uses` |

Any other operand fields not enumerated in `instruction_uses` are the same class
of defect: validator blind spots.


### Worked example — diamond (minimal pseudo-IR)

Illustrative only: names mirror BN IR ideas (`BlockId`, `ValueId`, `Branch` /
`Jump`). Not a parser grammar.

```text
CFG:
        B0
       /  \
     B1    B2
       \  /
        B3
```

```text
function @f entry=B0 {
  B0:
    %c = ...                    // condition
    branch %c then B1 else B2

  B1:
    %x = const 1                // defines %x on THIS path only
    jump B3

  B2:
    // intentionally does NOT define %x
    jump B3

  B3:
    %y = add %x, const 0        // USE of %x at join
    return %y
}
```

Suppose `function.blocks` is stored as `[B0, B1, B2, B3]` (common lowering order).

| Checker | What it does at B3 | Result |
| --- | --- | --- |
| **As-is** (`validate.rs`: one `HashSet`, scan blocks in vector order) | Visited B1 earlier → `%x` already in the set when B3 runs | **PASS** (wrong) — path B0→B2→B3 never defined `%x` |
| **CFG definite assignment** | `%x` must be defined on **every** predecessor edge into B3 (from B1 **and** B2), or an explicit merge/`φ` must define `%x` in B3 | **REJECT** (correct) — B2 has no definition |

Valid repairs under the **locked** φ rule:

1. Insert `%x = phi [B1:%x1, B2:%x2]` at the start of B3 (**required merge form**); or  
2. Avoid the merge in lowering (e.g. only `Load`/`Store` of a local — no SSA temp across the join); or  
3. Define and use only on paths that dominate (no join use).

**Negative fixture expectation:** constructing the diamond above (no merge, `%x`
missing on one arm) must fail language `validate` with a stable code once CFG
checking lands — today it may wrongly succeed.

**Related hole:** even a correct CFG walk fails if `instruction_uses` omits an
operand (e.g. `Input.prompt`): the use is invisible to the checker.


### Merge / φ form (**AQ-20 locked 2026-09-05**)

BN IR value ids are **single-assignment** (each defining instruction owns one
`ValueId`). A use at a join fed by **different** reaching definitions therefore
cannot reuse one id without an explicit merge.

**Decision:** the IR merge form is an explicit **`Phi`** instruction (name may be
`Instruction::Phi` / `Merge` in code):

- Operands: ordered list of `(predecessor BlockId, ValueId)` — one entry per
  predecessor edge that reaches the block.
- Result: a new `ValueId` defined at the start of the join block (before other
  ops in that block).
- `validate` treats `Phi` as defining its destination and **using** each incoming
  `ValueId` (those uses are checked on the respective predecessor paths).
- Until `Phi` exists in `ir/model`, CFG definite-assignment must **reject** any
  use whose reaching definitions are not unique/dominated (the diamond example).

**Non-goals for this lock:** full LLVM-style mem2reg narrative; memory
`Load`/`Store` locals remain valid for mutable variables — φ is for SSA values
that must merge across CFG edges.

### Acceptance (slice)

Before claiming “well-formed IR to both backends” for a release slice:

- [ ] CFG-based definite assignment (or equivalent SSA verify) implemented
- [ ] Negative fixtures: undefined on one branch of a diamond; loop-carried gap; join without merge
- [ ] `instruction_uses` complete for all operand-bearing instructions in the slice (incl. Input.prompt, Default.dynamic_dimensions)
- [x] Document which merge/`φ` form the IR uses — **explicit `Phi`** (AQ-20 locked)
- [ ] Implement `Instruction::Phi` + CFG validate + lowering emission where needed

## Well-formed IR handoff (acceptance — both backends)

**Approved promise:** after a successful Frontend job, **interpret** and
**compile** both consume BN IR that is **language-valid** (passed semantic
analysis obligations and **`validate`**). This is the highest-priority
verifiable claim before hard-split “done” — also summarized as **W1–W5** in
[`../../AGENTS.md`](../../AGENTS.md).

| Id | Requirement | Evidence |
| --- | --- | --- |
| **W1** | Backend entrypoints take **validated** IR only (no AST-as-meaning) | `ValidatedModule`; CLI uses `lower_graph_validated`, `execute_validated_with_host`, and `lower_validated_module_for_target`; `tests/validated_ir.rs` |
| **W2** | Ill-formed IR → **language** diagnostics, including **CFG definite assignment** and complete operand-use enumeration | Negative fixtures (**GC-IR**); see § Definite assignment |
| **W3** | Same validated IR for interpret and compile of one job | Single lower+validate; compile does not re-lower from AST |
| **W4** | Target gaps → **support** diagnostics via `validate_for` | Distinct codes (**GC-SUP**); not reported as W2 failures |
| **W5** | `bn_ir` public model free of FE/semantic types | **GC-DEP**; do not grow `semantic::{Type,SymbolId}` in IR |

Until W2/W5 have automated evidence for the release slice, do not advertise
“both backends receive semantically well-formed IR” as satisfied.

## Checklist — what the full contract must eventually define

- [ ] **Module / program shape** — units, linkage of modules, entry, how imports appear in IR.
- [ ] **Ops / instruction set** — complete op catalog (kinds, operands, side-effect notes) aligned with `ir/model` (and crate `bn_ir` after split).
- [ ] **Types** — IR type system vs language types; lowering rules; HOST value boundaries. IR types must be **IR-owned** (not re-exports of `semantic::Type`).
- [ ] **Validation rules** — structural / language IR checks performed by **`validate`** (3.2); error codes into **D3**.
- [ ] **Definite assignment / CFG** — every use defined on all executable paths; complete `instruction_uses`; see § above (priority for well-formed IR claim).
- [ ] **Target-support check (separate)** — `validate_for(target)` / matrix lookup; failures use a **support** diagnostic family (e.g. `TARGET_UNSUPPORTED_*`), **not** language-invalid IR. An LLVM gap must not look like a language error. See [support-matrix.md](support-matrix.md).
- [ ] **Versioning** — IR format / schema version; compatibility expectations across toolchain releases.
- [ ] **LLVM subset matrix** — which BN IR ops are supported for compile vs interpret-only; see [support-matrix.md](support-matrix.md) (structured catalog; EXAMPLE rows are not coverage). Unsupported-for-llvm must fail the **support check** / `validate_for`, with a stable **support** diagnostic — not `validate` (bucket 0.4.5 G2 / XM10).
- [ ] **Diagnostics on FE→IR path** — lower/validate use `Diagnostic` (no free-form `String` on the contract boundary).
- [ ] **Source identity** — spans / debug locs carry **SourceId** (and revision where published); types live in shared **`bn_source` leaf** (not inside frontend); lowering preserves identity — [frontend-session.md](frontend-session.md).
- [ ] **Invariants for backends** — interpret and compile consumption rules; no AST fork; HostEnv vs `bn_rt` boundaries.

## Non-goals for this stub

- Does not replace the language EBNF or `0.4.md`.
- Does not specify Fluent catalogs or CLI UX (`bnc` options).
- Does not freeze crate layout; it only names the contract surface backends must share.

## See also

- [semantic-analysis.md](semantic-analysis.md)

- [Architecture README](README.md)
- [target-architecture.md](target-architecture.md)
- [milestones-map.md](milestones-map.md)
- [support-matrix.md](support-matrix.md) — verifiable support-matrix contract
