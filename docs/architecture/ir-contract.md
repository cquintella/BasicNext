# BN IR contract (to-be) — release-slice inventory

> Canonical: `docs/architecture/ir-contract.md`  
> **Status:** **release-slice inventory in progress** (updated 2026-09-06).
> Handoff **W1**, CFG validation, explicit `Phi`, and an initial pre-emit
> `validate_for` are in code. The operation inventory below is synchronized with
> historically `src/ir/model.rs` — now `crates/bn_ir`; support coverage and final
> diagnostic catalog remain open. See [review-status.md](review-status.md),
> [to-close.md](to-close.md).

## Purpose

Define the **single Intermediate Representation (BN IR)** that sits between the Frontend (Analyze Sources → Lower and Validate IR) and the Backend legs **interpret** and **compile**. One IR for the job; both backends consume it from store **D2**.

## Normative posture (locked direction)

### `validate` vs target support (locked 2026-09-05)

- **`validate`**: “Is this IR valid **as BN**?” Failure ⇒ language/IR defect.
- **Support check / `validate_for(target)`**: “Does **this target** implement this IR under the [support-matrix.md](support-matrix.md)?” Failure ⇒ valid program, unsupported here.
- Do not collapse the two. Matrix rows must name `reject_diag` + `tests`.

**Production-complete `validate_for` (required — not a stub):**

- Must run on every compile path before llvm emit.
- Failures use only `TARGET_UNSUPPORTED_*` (or catalog ids), never language `INVALID_IR_*`.
- Must cover the same rejects users hit today via `BUILD_LOWERING_UNAVAILABLE` /
  `unsupported_instruction`, or those sites must be removed.
- Claimed support subset must have matrix evidence + fixtures (see [to-close.md](to-close.md)).

**API shape:**

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

**0.4.5 migration procedure (AQ-15):** define IR-owned types and identities,
add explicit frontend conversions and graph-scoped ID remapping during lowering,
migrate validation and both consumers, then remove old imports/re-exports before
extracting crates. Shared source identities stay in the source leaf. Test
multi-module collisions and type conversion; a semantic type alias under an IR
name does not satisfy GC-DEP.


- **Interpret is the executable reference** (subordinate to the language specification). Running a program means executing validated BN IR under HostEnv — not LLVM `lli`, and not a private AST interpreter. Compiler equivalence is judged against the **spec** (and the support matrix), not against “whatever the interpreter did when buggy.” See [conformance.md](conformance.md).
- **Compile lowers the same IR.** Compile reads the same validated BN IR and lowers BN IR → LLVM IR for the external clang/ld/opt toolchain. It must not invent a second meaning from AST alone.
- Frontend produces AST + symbols; **2.5** must satisfy [semantic-analysis.md](semantic-analysis.md). **Lowering** (AST+semantic → BN IR) is a **Frontend** responsibility; **`bn_ir` must not depend on `bn_frontend`/semantic**. Process **3.0** in the DFD is the logical lower+validate stage: lower runs in the frontend crate, **validate** is the IR crate’s job. Backends consume validated IR only and do not re-lower from AST.
- **As-is debt (updated 2026-09-06):** IR lives in `crates/bn_ir` on `bn_types` / IR-owned `SymbolId`/`ModuleId` — **no** `semantic::` imports. Remaining GC-DEP debt is **packaging** (path-shim frontend, thin-`bn`), not semantic leakage into the IR model.

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

### Validator status (`src/ir/validate.rs`) — synced 2026-09-05

**Done (0.4.4 S1.2):** reachable-CFG **must-definition** via predecessor/successor
graphs and a fixed-point over per-block incoming/outgoing defined sets (not
block-list order). `instruction_uses` includes `Input.prompt` and
`Default.dynamic_dimensions`. Negative/positive fixtures:
`tests/validated_ir.rs` (diamond one-branch use; undefined prompt; undefined
dynamic dimension).

**Still open:**

| Item | Status |
| --- | --- |
| Full operand-field inventory on every `Instruction` variant | Complete and compiler-enforced by the exhaustive `instruction_uses` match; new indexed field/static stores are covered |
| Explicit `Instruction::Phi` in `ir/model` + lowering | AQ-20 **implemented** for the scalar subset; target-specific coverage remains matrix-owned |
| Formal op catalog + IR-owned type system (no `semantic::`) | Types moved to `bn_types` / `bn_ir`; **op catalog** still incomplete; packaging GC-DEP remains |
| Historical “block-order HashSet” bug | **Fixed** — do not cite as current behaviour |

The diamond worked example below is the **acceptance case**; the CFG checker is
expected to **reject** it today (see `validator_rejects_value_defined_on_only_one_branch`).


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
| **Historical bug** (block-list `HashSet`) | Visited B1 earlier → `%x` already in the set when B3 runs | **PASS** (wrong) |
| **Current CFG must-definition** | `%x` must be defined on **every** predecessor path into B3, or an explicit `Phi` | **REJECT** (required; fixture exists) |

Valid repairs under the **locked** φ rule:

1. Insert `%x = phi [B1:%x1, B2:%x2]` at the start of B3 (**required merge form**); or  
2. Avoid the merge in lowering (e.g. only `Load`/`Store` of a local — no SSA temp across the join); or  
3. Define and use only on paths that dominate (no join use).

**Negative fixture:** the diamond above (no merge, `%x` missing on one arm) is
covered by `tests/validated_ir.rs` and must **fail** language `validate`.

**Related hole (mitigated for known cases):** `Input.prompt` /
`Default.dynamic_dimensions` are now in `instruction_uses`; keep auditing other
operand fields so no use stays invisible.


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

- [x] CFG-based definite assignment MVP implemented (`src/ir/validate.rs` fixed-point)
- [x] Negative fixtures seed: diamond one-branch; undefined prompt; undefined dynamic dimension (`tests/validated_ir.rs`)
- [ ] Broader negative set (loop-carried gap; richer joins) as slice grows
- [x] Full exhaustive `instruction_uses` audit, including `Input.prompt`, `Default.dynamic_dimensions`, and every indexed-store operand
- [x] Document which merge/`φ` form the IR uses — **explicit `Phi`** (AQ-20 locked)
- [x] Implement `Instruction::Phi` + validate/lowering emission where needed for the scalar LLVM subset; target-specific unsupported cases remain matrix-owned.

## Release-slice operation inventory

This table is normative for the current in-tree IR shape. `result` identifies
the single SSA destination, `uses` lists every `ValueId` consumed by the
operation (including collection fields), and `effect` records observable or
stateful behavior. It is intentionally independent of target support: an
operation can be valid BN IR while remaining unsupported by a backend.

| Operation | Result | Uses | Effect / validation notes |
| --- | --- | --- | --- |
| `Constant` | `destination` | — | Pure literal or host/module/function/type token; `ty` must match `value` (including host/module/function token forms). |
| `Default` | `destination` | `dynamic_dimensions` | Allocates a typed default value; dimensions are non-negative and bounded. |
| `Phi` | `destination` | one value per incoming edge | Pure SSA merge; incoming edges must exactly equal reachable predecessors and each value must be defined on its predecessor path. |
| `Load` | `destination` | — | Reads a symbol slot; symbol/type relationship is frontend-owned metadata. |
| `Store` | — | `value` | Writes a symbol slot; `ty` describes the stored value. |
| `Copy` | `destination` | `source` | Pure value copy with type compatibility. |
| `Unary` | `destination` | `operand` | Pure checked unary operation; operator/type are validated by the language layer. |
| `Binary` | `destination` | `left`, `right` | Pure checked binary operation; operator/type are validated by the language layer. |
| `Cast` | `destination` | `value` | Checked language conversion; target support is separate. |
| `Call` | `destination` | `callee`, `arguments` | Calls a BN function value; may observe or mutate host/program state. |
| `DispatchSubmit` | `destination` | `callee`, `queue`, `task`, `arguments` | Submits asynchronous work; dispatch policy and handle validity are runtime concerns. |
| `DispatchAwait` | `destination` | `callee`, `ticket`, `timeout` | Waits for asynchronous work; timeout and ticket errors are runtime diagnostics. |
| `Input` | `destination` | optional `prompt` | Reads host input; prompt is a complete operand when present. |
| `Vector` | `destination` | `values` | Constructs a vector; all elements are operands and dimensions/types must agree. |
| `Index` | `destination` | `object`, `index` | Reads an indexed value; bounds/type checks are language/runtime rules. |
| `Member` | `destination` | `object` | Reads a member; owner/name identity must resolve. |
| `SetIndex` | — | `indices`, `value` | Mutates indexed storage; every index and the assigned value are operands. |
| `SetMemberIndex` | — | `object`, `indices`, `value` | Mutates indexed storage owned by an object identity; the receiver and at least one integer index are required. |
| `SetFieldIndex` | — | `indices`, `value` | Mutates an indexed field path rooted in a binding, preserving struct value semantics. |
| `SetStaticIndex` | — | `indices`, `value` | Mutates indexed static storage after class initialization. |
| `Length` | `destination` | `vector` | Pure shape/length query. |
| `SizeOf` | `destination` | `value` | Pure static-size query for the value/type model. |
| `Print` | — | `values` | Console effect; every printed value is an operand. |
| `ClearScreen` | — | `console` | Console effect; execution policy is rechecked at the boundary. |
| `Beep` | — | `console` | Console effect; execution policy is rechecked at the boundary. |
| `Allocate` | `destination` | `arguments` | Allocates a class/object value; class identity and constructor arguments must resolve. |
| `Delete` | — | `value` | Releases an object/pointer and optionally invokes its destructor. |
| `SetMember` | — | `object`, `value` | Mutates an object member; owner/name/type must resolve. |
| `EnsureClass` | — | — | Ensures static class initialization; class identity must resolve. |
| `LoadStatic` | `destination` | — | Reads a static class field; class/field/type must resolve. |
| `StoreStatic` | — | `value` | Writes a static class field; class/field/type must resolve. |

`Module.class_bases` carries fully qualified direct-base identities. It is
language IR metadata, has no frontend type dependency, must be acyclic, and
allows backends to lay out inherited fields in base-to-derived order.

The control-flow terminators are `Jump { target }`, `Branch { condition,
then_block, else_block }`, `Return { value }`, and `Stop { code }`. `Jump` and
`Branch` define CFG edges; `Branch.condition`, `Return.value` when present, and
`Stop.code` are operands. Block IDs must be dense and ordered in the current
in-memory representation, and all referenced targets must exist.

The current `Constant` alternatives are `Integer`, `Float`, `String`,
`Boolean`, `Null`, `NotAvailable`, `EndOfFile`, `Function`, `Type`,
`HostConsole`, and `HostArgs`. There is no serialized-IR compatibility promise
in 0.4.5; the in-memory model version is the toolchain release boundary until
an explicit schema/versioning decision is accepted.

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
| **W5** | `bn_ir` public model free of FE/semantic types | **Met in model** (2026-09-06); keep regressions out; packaging/shim debt tracked in bucket C-P0.3 / C-P1.5 |

Until W2/W5 have automated evidence for the release slice, do not advertise
“both backends receive semantically well-formed IR” as satisfied.

## Checklist — remaining contract closure

- [ ] **Module / program shape** — units, linkage of modules, entry, how imports appear in IR.
- [x] **Ops / instruction set** — release-slice catalog (kinds, operands, side-effect notes) aligned with `ir/model`; re-audit is required whenever the model changes.
- [X] **Types (ownership)** — IR types via `bn_types` / IR-owned ids (not `semantic::Type`). Remaining: full catalog + HOST boundaries prose.
- [ ] **Validation rules** — structural / language IR checks performed by **`validate`** (3.2); error codes into **D3**. The current slice rejects incompatible constant categories, copies, vector elements, and indices.
- [ ] **Definite assignment / CFG** — every use defined on all executable paths; complete `instruction_uses`; see § above (priority for well-formed IR claim).
- [x] **Target-support check (separate)** — `llvm::validate_for` runs after language validation on native/wasm emission paths and returns `TARGET_UNSUPPORTED_*`; complete matrix lookup remains open. An LLVM gap must not look like a language error. See [support-matrix.md](support-matrix.md).
- [x] **Versioning** — 0.4.5 explicitly makes no serialized-IR compatibility promise; an in-memory model version is the toolchain release boundary. A serialized schema remains future work.
- [ ] **LLVM subset matrix** — which BN IR ops are supported for compile vs interpret-only; see [support-matrix.md](support-matrix.md) (structured catalog; EXAMPLE rows are not coverage). Unsupported-for-llvm must fail the **support check** / `validate_for`, with a stable **support** diagnostic — not `validate` (bucket 0.4.5 G2 / XM10).
- [ ] **Diagnostics on FE→IR path** — lower/validate use `Diagnostic` (no free-form `String` on the contract boundary).
- [ ] **Source identity** — spans / debug locs carry **SourceId** (and revision where published); types live in shared **`bn_source` leaf** (not inside frontend); lowering preserves identity — [frontend-session.md](frontend-session.md).
- [ ] **Invariants for backends** — interpret and compile consumption rules; no AST fork; HostEnv vs `bn_rt` boundaries.

## Non-goals for this release-slice contract

- Does not replace the language EBNF or `0.4.md`.
- Does not specify Fluent catalogs or CLI UX (`bnc` options).
- Does not freeze crate layout; it only names the contract surface backends must share.

## See also

- [semantic-analysis.md](semantic-analysis.md)

- [Architecture README](README.md)
- [target-architecture.md](target-architecture.md)
- [milestones-map.md](milestones-map.md)
- [support-matrix.md](support-matrix.md) — verifiable support-matrix contract
