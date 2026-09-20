# Bucket 0.5.2.1b — Fragilidade 6: positional records and shared immutable values

**Status:** Opened 2026-09-19 (corrective planning; no implementation started).  
**Depends on:** 0.5.2 shipped (`23550aa`) and the 0.5.2a runtime/policy
correction closed (`f973498`).  
**Gate:** Carlos. Sprints execute sequentially; a later sprint must not start
while an earlier acceptance gate is open.  
**Source:** [`advisory-16-09-26.md`](../advisory-16-09-26.md), Fragilidade 6.  
**Tag policy:** no independent tag is implied by this planning bucket. If the
work ships as a patch, Carlos chooses the release number before the closing
sprint and the release note uses that number.  
**WBS:** no 0.5.2.1 WBS exists. This bucket is the source of truth for the
corrective iteration; implementation progress must be recorded here in the
same change as its evidence.

**Spec check:** this is an internal representation and IR-contract correction,
not a language change. `STRING`, `STRUCT`, class-field, `Error`, and `BNJson`
observable behaviour remain governed by `docs/language/0.5/0.5.md`,
`docs/library/error.md`, and `docs/language/0.3/bnjson.md`. Grammar, keywords,
HOST capability catalog, and source syntax do not change. SPRINT 0 must stop
at a decision gate if those sources cannot describe the intended observable
behaviour without amendment; implementation must not invent a new rule.

---

## SECTION 0 — Verified starting point (2026-09-19, audited at `f973498`)

The audit was read-only. The working tree already contained unrelated user
changes, so the paths below are authoritative but line numbers are deliberately
not frozen.

| # | Evidence | Consequence |
| --- | --- | --- |
| 1 | `crates/bn_value/src/lib.rs`: `Value::Record { type_name: String, fields: HashMap<String, Value> }` | Every interpreted field read/write hashes a string although semantic analysis already resolved the member. Record clone also clones keys and hash-table storage. |
| 2 | `bn_ir::SymbolId` identifies bindings, not record fields. `bn_value` depends only on `bn_types`; `bn_ir` and `bn_value` are sibling lower-layer crates. | Reusing `SymbolId` inside `Value` would conflate identities and create the wrong crate dependency. The IR needs a distinct field identity; the runtime value needs only positional storage. |
| 3 | `Instruction::{Member, SetMember, SetMemberIndex}` carry `name: String`; `SetField` and `SetFieldIndex` carry `path: Vec<String>`. | Lowering discards the analyzer's successful resolution and both backends repeat name-to-layout work. Ill-formed field references cannot be fully validated as positional references. |
| 4 | `bn_llvm::llvm::layout::class_layout_fields` reconstructs layout by scanning `FieldInit`/`Default` functions for `SetMember`. | LLVM and the interpreter do not consume one explicit, validated record-layout contract. Layout is inferred from executable instructions. |
| 5 | Record access and mutation by name occur in `bn_interp`, `bn_host_net`, and `bn_lib_web`; retain/release, equality, nested updates, and `LEN` also traverse record maps. | The migration is cross-crate and must preserve ownership, nested-struct value semantics, host-provider projections, and backend parity. |
| 6 | `Value::String(String)` and other immutable textual payloads clone owned buffers. The `HashMap` record variant currently dominates `Value` size. | Shared immutable text should use `Arc<str>`; representation size and clone behaviour require direct tests rather than assumption. |
| 7 | `crates/bn_lib_json/src/json.rs` defines a private JSON `Value` enum even though `serde_json` is already a root dependency. `bn_lib_json` does not yet depend on it directly. | The duplicate enum can be removed, but the bounded BNJson contract must survive: 8 MiB input/output, depth 64, duplicate-key rejection, trailing-input rejection, and Unicode correctness. |
| 8 | `Value::Integer(i128, IntegerType)` and `Value::Error { code, message }` remain broader design concerns in the advisory, but neither is included in its corrective list (a)–(d). | Integer specialization and a language-visible redesign of `Error` are explicit non-goals. `Error.Message` may adopt shared immutable storage without changing `Code`/`Message` behaviour. |

### 0.1 Scope locked for this bucket

1. Intern field names once in BN IR and make record layouts explicit and
   validator-owned.
2. Resolve field references and nested field paths during analysis/lowering;
   backends consume validated positional references.
3. Represent interpreted records as a type identity plus `Box<[Value]>`, with
   no `HashMap<String, Value>` and no name hashing on record access.
4. Represent immutable runtime text with `Arc<str>` where it is carried by
   `Value`.
5. Remove the private BNJson value enum in favour of `serde_json::Value` while
   preserving the accepted bounded contract.
6. Add functional, negative, parity, representation-size, and reproducible
   performance evidence.

### 0.2 Authoritative Fragilidade 6 mapping

This bucket addresses **Fragilidade 6 — interpreter value model**, specifically
the four corrective measures prescribed by the advisory. The implementation
may refine the proposed identity type when the current architecture proves the
literal suggestion unsafe, but it must deliver the same outcome.

| Advisory correction | Bucket delivery | Acceptance owner |
| --- | --- | --- |
| **(a) Intern field names; the advisory points to `SymbolId` in `bn_ir`.** | SPRINT 2 introduces an IR-owned field identity and one interned name table. It must be a distinct `FieldId`, because the current `SymbolId` is a binding identity; reusing it would conflate contracts. | D-V-01; activities 2.1–2.3; V1–V3/V8 |
| **(b) `Box<[Value]>` records with field index resolved in analysis/lowering.** | SPRINT 2 emits and validates canonical layouts/resolved slots; SPRINT 3 replaces the runtime map with `RecordValue { fields: Box<[Value]> }` and migrates every consumer. | D-V-02/03; activities 2.2–3.5; V1–V5 |
| **(c) `Arc<str>` for shared immutable strings.** | SPRINT 1 adds one `SharedString = Arc<str>` convention for immutable textual `Value` payloads and proves allocation-free clones. | D-V-04; activities 1.1–1.2; V6 |
| **(d) Delete the parallel JSON `Value` in favour of `serde_json::Value`.** | SPRINT 1 removes the private enum while preserving every bounded BNJson rule, including duplicate-key rejection that plain `serde_json::from_str` would not preserve. | D-V-05; activities 1.3–1.4; V7 |

The evidence paragraph also identifies `Integer(i128, IntegerType)` and the
stringly internal `Error` representation, but the advisory's corrective list
does not prescribe an integer rewrite or a new public `Error` model. This
bucket therefore changes `Error.Message` storage under measure (c) while
preserving the language-visible `Error { Code, Message }` contract. Integer
specialization remains separately measurable debt and **must not be claimed as
fixed** when this bucket closes.

### 0.3 Explicit non-goals

- No source-language syntax, grammar, keyword, HOST surface, or public BNJson
  API change.
- No integer-width representation rewrite and no rewrite of checked integer
  arithmetic.
- No change to the language-visible `Error { Code, Message }` contract.
- No HIR project, general-purpose global string interner, LLVM typed-builder
  revival, object-heap redesign, or unrelated executor file split.
- No benchmark-only unsafe code, no mocks, and no weakening/removal of coverage
  to make a slow test appear efficient.

---

## SECTION 1 — Target contract and decisions

### 1.1 Layering and identities

The planned boundary is:

```text
bn_frontend semantic resolution
        │
        ▼
bn_frontend lowering
        │ produces explicit field table/layout/reference
        ▼
bn_ir::Module ── validate ──► ValidatedModule
        │                         │
        ├─────────────────────────┼────────► bn_llvm
        │                         └────────► bn_interp
        ▼
bn_value::RecordValue { type_name: Arc<str>, fields: Box<[Value]> }
        (no dependency on bn_ir; positional storage only)
```

`bn_ir` owns a distinct `FieldId`; it must not reuse binding `SymbolId`.
Lowering emits an interned field-name table, ordered layouts per record/class,
and resolved references containing enough owner/slot information for
`validate` to reject a mismatched owner, unknown id, duplicate slot, wrong
field type, or invalid nested path. `bn_value` does not import `FieldId`; the
validated interpreter projects a resolved field reference to a checked
`usize` before indexing `RecordValue`.

### 1.2 Required invariants

- **V1 — one layout:** lowering is the only producer of ordered record layouts;
  `validate` checks them; neither backend reconstructs them by scanning
  executable instructions.
- **V2 — complete layout:** every non-static field appears exactly once in
  declaration order; inherited fields precede derived fields; a descendant
  cannot change an ancestor slot.
- **V3 — resolved access:** every record-member read/write and every segment of
  a nested field path names a valid field identity and slot in the applicable
  layout before a backend receives the IR.
- **V4 — bounded indexing:** conversion from the IR slot to `usize` is checked;
  a positional miss is `INVALID_IR`, never a panic or silently created field.
- **V5 — value semantics:** nested `STRUCT` updates, copies, equality,
  retain/release traversal, weak-field handling, and `LEN` retain their current
  observable behaviour.
- **V6 — immutable text:** cloning a runtime text value shares its allocation;
  operations that produce new text allocate a new immutable value.
- **V7 — BNJson compatibility:** changing the internal JSON value cannot relax
  or alter the documented size, depth, duplicate-key, trailing-input, Unicode,
  parse, stringify, release, or use-after-release behaviour.
- **V8 — architecture:** `bn_value` remains free of `bn_ir` and `bn_frontend`;
  `bn_ir` remains free of `bn_frontend`; W1–W5 and GC-IR/GC-DEP remain true.

### 1.3 Decision gates (accepted 2026-09-20)

| ID | Accepted contract | Reject if |
| --- | --- | --- |
| **D-V-01** | `bn_ir` owns module-local `FieldId(u32)` and `FieldSlot(u32)`. A resolved `FieldRef` carries `owner`, `id`, and `slot`; nested paths are `Vec<FieldRef>`. The module field-name table is the only id→spelling source. | It reuses binding `SymbolId`, makes `bn_value` depend on `bn_ir`, repeats a field spelling in runtime values, or leaves a backend to resolve an unresolved name. |
| **D-V-02** | `bn_ir::Module` owns ordered layouts by qualified owner. Each layout entry carries field id, declared type, declaring owner, and weak ownership. Lowering builds base-first layouts from analyzed declarations, including imported/host record types that cross the provider seam. Existing `class_bases` remains for inheritance/dispatch; duplicate `weak_fields` metadata is removed after consumers migrate. | Layout is inferred from executable `SetMember`, backend-specific, incomplete for provider records, or absent for a valid record type. |
| **D-V-03** | `bn_value::RecordValue` owns `type_name: SharedString` and `fields: Box<[Value]>`. Its public API is `new`, `type_name`, `len`, `is_empty`, checked `get`/`get_mut`/`replace`, ordered `iter`/`iter_mut`, and `into_fields`; it accepts numeric slots only and returns `Option` at bounds. `bn_interp` maps a missing slot to `INVALID_IR`. | A public helper accepts a field name, indexes unchecked, hides a hash map, or imports an IR identity. |
| **D-V-04** | `pub type SharedString = Arc<str>` plus `shared_string(value: impl Into<SharedString>)`. Every immutable textual `Value` payload uses it: BN string, function/type names, error message, handle/record/object type identity, and timezone. Mutable construction uses local `String` and converts once at the value boundary. | The migration creates parallel owned/shared conventions or converts to/from `String` on ordinary reads/clones. |
| **D-V-05** | `bn_lib_json` stores `serde_json::Value`. A serde `DeserializeSeed`/visitor (or equivalently bounded adapter proven by the same tests) enforces maximum depth 64 and duplicate-key rejection; byte limits are checked at input/output boundaries. | Plain `serde_json::from_str` silently accepts a duplicate key, or any documented bound/error class changes. |

Carlos's 2026-09-20 “continue” after review of the bucket accepts D-V-01..05 as
specified above. Any implementation discovery that invalidates one of these
contracts reopens the decision before the dependent production change.

---

## SECTION 2 — Test execution and timing protocol (all sprints)

Tests are run when the current development activity needs their evidence, not
speculatively while planning. Every executed test command is timed and recorded
under the activity or in the evidence table in SECTION 8.

### 2.1 Timing procedure

1. Run the smallest relevant test first, wrapped with a portable elapsed-time
   measurement (`/usr/bin/time -p` where available; otherwise the shell's
   `time` with the exact method recorded).
2. Record command, layer (unit/integration/parity/full), real elapsed seconds,
   result, build-cache state (cold/warm), and date/commit.
3. A command at or below **5.0 s** needs no performance investigation.
4. If a scoped test command takes more than **5.0 s**, rerun with the narrowest
   available filters to identify individual tests. Record every individual
   test above **5.0 s**. Do not attribute compilation time to a test body.
5. For each confirmed slow test, create or update a numbered activity in
   SECTION 7 with cause, coverage owned by that test, proposed faster layer or
   fixture, and an equivalence argument showing that coverage is preserved.
6. A slow test does not block the functional sprint merely because it is slow;
   an unexplained regression, nondeterminism, or avoidable wait does block the
   efficiency gate. Never delete, skip, mock away, or weaken a case to meet the
   threshold.
7. Re-run an optimized test at least three warm times and record the median.
   Acceptance requires the same assertions/contract coverage and no new flake.

### 2.2 Test strategy

| Layer | Subject | Real collaborators | Required cases |
| --- | --- | --- | --- |
| Unit | IR field table/layout validator | Real `bn_ir` model | valid base/derived/struct layouts; unknown id; duplicate id/slot; slot/name/owner/type mismatch; malformed nested path; out-of-range slot |
| Unit | `RecordValue` and shared text | Real `bn_value` | construction; checked access/mutation; clone/drop; nested records; zero/one/many fields; `Arc::ptr_eq`; 64-bit representation-size budget |
| Unit | BNJson bounded adapter | Real `serde_json::Value` | accepted round-trip; 8 MiB boundaries; depth 64/65; duplicate key; trailing input; surrogate pair; invalid control/UTF-8 path; output bound |
| Integration | frontend → lower → validate → interpret | Real frontend, IR, interpreter and providers | struct/class/inheritance reads and writes; nested updates; host Net/Web record projections; ownership traversal; diagnostics |
| Cross-backend | same validated module → interpret/native | Real interpreter, LLVM, `bn_rt` | existing record/struct/object fixtures plus boundary/error/effect families applicable to the touched operations |
| Broad close | workspace and architecture gates | Real crates/scripts | regression only; no mock substitutes for lower-level evidence |

---

## SECTION 3 — SPRINT 0: freeze contract, baseline, and failing tests

- [X] **0.1 ACTIVITY DONE — confirm D-V-01..05.** Update SECTION 1 with the
  accepted exact types and public methods before production code changes.
  **Objective/value:** prevent a representation optimization from creating a
  new dependency cycle or an IR contract that one backend interprets
  differently. **Dependencies:** none. **Acceptance:** each decision is
  accepted/amended; no unresolved layout or BNJson-contract question remains.
  **Evidence (2026-09-20):** exact types/APIs recorded in §1.3; Carlos continued
  execution after reviewing the bucket.
- [X] **0.2 ACTIVITY DONE — audit normative behaviour.** Read the current 0.5
  language sections for strings, structs/classes, inheritance, ARC and `Error`,
  plus `docs/language/0.3/bnjson.md`. Record either “no language amendment” in
  History or stop with the exact normative gap. **Acceptance:** no observable
  behaviour is inferred from current implementation accidents. **Evidence
  (2026-09-20):** `0.5.md` fixes string immutability, inheritance/field
  uniqueness, struct aggregate lifetime and shared validated-IR parity;
  `docs/library/error.md` fixes public `Code`/`Message`; the BNJson 0.3 contract
  fixes the 8 MiB/depth/error boundaries. No language amendment is required.
- [ ] **0.3 ACTIVITY TODO — capture structural and size baseline.** Measure the
  current 64-bit `size_of::<Value>()`, record allocation shape, and owned-string
  clone cost in a scratch harness without adding a change-detector unit test.
  Immediately before SPRINT 1/3 implementation, add behavioural tests first;
  the final representation test targets `size_of::<Value>() <= 48` bytes on
  64-bit targets. **Acceptance:** baseline command/result is recorded in
  SECTION 8; the permanent test is observed RED immediately before the change
  that makes it GREEN.
- [ ] **0.4 ACTIVITY TODO — specify the IR negative matrix.** Record concrete
  malformed field-table/layout/reference/path cases and their expected
  `INVALID_IR` facts. Write each executable test as the first RED action of
  SPRINT 2, immediately followed by its minimal GREEN implementation; never
  leave the repository intentionally red between sprints. **Acceptance:** the
  matrix covers V1–V4 with stable source spans and no panic expectation.
- [ ] **0.5 ACTIVITY TODO — establish functional fixtures.** Identify or add the
  smallest `.bn` fixtures for empty/single/multi-field structs, inheritance,
  nested field update, vector field, record equality, ARC-held object field,
  and host-provider records. Capture unchanged expected output/diagnostics for
  both claimed backends. **Acceptance:** fixtures describe language behaviour,
  not internal maps or slots.
- [ ] **0.6 ACTIVITY TODO — capture performance baseline.** Run a scratch,
  release-mode microbenchmark using the current real `HashMap<String, Value>`
  and owned-string operations: at least one million reads/writes across 1, 8,
  and 32 fields plus one million string clones. Use seven samples and report
  the median; the tracked ignored harness is introduced test-first with the
  positional API in SPRINT 3. **Acceptance:** command, machine/toolchain
  context, medians, and allocations are recorded in SECTION 8.

**Definition of Ready:** D-V-01..05 are closed; normative behaviour, negative
matrix, fixtures and baselines are known; each later implementation activity
names the RED test it will run first.

**Verification SPRINT 0 (timed per SECTION 2):** focused `bn_value` and `bn_ir`
tests, fixture baselines on currently supported backends, and
`git diff --check`. No full workspace run is required for test scaffolding.

---

## SECTION 4 — SPRINT 1: shared immutable text and bounded serde JSON

- [ ] **1.1 ACTIVITY TODO — introduce shared text.** In `bn_value`, add
  `SharedString = Arc<str>` and migrate immutable textual `Value` payloads
  named by D-V-04. Provide explicit constructors/conversions so callers do not
  scatter `Arc::<str>::from` policy. **Acceptance:** `Value::String` clone is
  allocation-free and `Arc::ptr_eq` proves sharing; concatenation/case
  conversion creates a distinct immutable value; observable rendering is
  unchanged.
- [ ] **1.2 ACTIVITY TODO — migrate consumers by responsibility.** Update
  `bn_interp`, HOST/provider crates, root adapters, and tests to consume
  `SharedString` without converting back to owned `String` except at an actual
  mutable/FFI boundary. Update every touched Rust file header to match its
  responsibility. **Acceptance:** no `unsafe`; no blanket `.to_string()` used
  merely to satisfy the migration; affected unit/integration tests are green.
- [ ] **1.3 ACTIVITY TODO — replace the private JSON enum.** Add
  `serde_json.workspace = true` (or the repository-equivalent exact version) to
  `bn_lib_json`, store `serde_json::Value`, and remove
  `crates/bn_lib_json/src/json.rs`'s private `enum Value`. Keep a focused bounded
  adapter module if required by D-V-05. **Acceptance:** only
  `serde_json::Value` represents JSON documents in the provider.
- [ ] **1.4 ACTIVITY TODO — preserve BNJson negative boundaries.** Write the
  tests listed in SECTION 2 before replacing the implementation. Duplicate
  object keys must still fail rather than use serde_json's ordinary last-value
  behaviour; depth 65 and inputs over 8 MiB fail; depth 64 and exact-boundary
  valid documents follow the normative contract. **Acceptance:** unit tests and
  the real `tests/runtime.rs` BNJson provider integration pass without mocks.

**Definition of Done:** V6 and V7 hold; the duplicate JSON enum is gone; no
language/API change; focused tests and timing evidence are recorded; every
confirmed >5 s test has a SECTION 7 disposition.

**Verification SPRINT 1 (timed):** `cargo test -p bn_value`,
`cargo test -p bn_lib_json`, the filtered BNJson runtime integration, affected
provider tests, `cargo fmt --check`, and `cargo clippy` for the touched crates
with `-D warnings`.

---

## SECTION 5 — SPRINT 2: explicit validated field layouts in BN IR

- [ ] **2.1 ACTIVITY TODO — add the IR-owned field contract.** Implement the
  accepted D-V-01/D-V-02 types in focused `bn_ir` modules (model types separate
  from validation if either file would exceed 500 lines). Re-export through
  `lib.rs`; keep `lib.rs` free of business logic. **Acceptance:** public docs
  state module-local identity and slot invariants; `bn_ir` dependencies remain
  free of frontend/runtime/value crates.
- [ ] **2.2 ACTIVITY TODO — validate tables, layouts and references.** Make the
  red tests from 0.4 green, including complete operand/use enumeration for all
  modified instructions. **Acceptance:** GC-IR/W2: malformed layouts and every
  malformed member/path use are rejected with stable language-IR diagnostics;
  `validate` does not perform target-support classification.
- [ ] **2.3 ACTIVITY TODO — lower canonical layouts.** Extend semantic results
  only as narrowly as needed, then make lowering intern names and emit ordered
  layouts for structs/classes, bases before derived fields, imported qualified
  owners, weak fields, and zero-field records. Resolve member instructions and
  every nested path to the accepted IR field-reference type. **Acceptance:** no
  backend sees unresolved textual field access for record/class storage.
- [ ] **2.4 ACTIVITY TODO — consume layouts in LLVM.** Replace
  `class_layout_fields` instruction scanning with validated module layout data;
  compute offsets, ownership traversal, vector-field offsets, and member
  emission from that data. Preserve special non-record projections such as
  `Error` and `HOST.Exec.Result` explicitly; do not force them into a record
  layout accidentally. **Acceptance:** LLVM has no layout reconstruction from
  `FieldInit`/`Default` instructions; GC-SUP remains separate from GC-IR.
- [ ] **2.5 ACTIVITY TODO — document the IR contract.** Update
  `docs/architecture/ir-contract.md` and `value-memory-abi.md` with field
  identity, layout order, validation ownership, and backend handoff. Update
  completion/conformance docs only if an existing GC description would
  otherwise be incomplete. **Acceptance:** docs describe the implemented
  contract and cite its negative/parity tests.

**Definition of Done:** V1–V4 and V8 hold for the new IR; W1–W5 remain true;
both backends consume the same validated layout; focused validator, lowering,
and LLVM tests are green and timed.

**Verification SPRINT 2 (timed):** `cargo test -p bn_ir`, focused frontend
lowering tests, `cargo test -p bn_llvm`, applicable codegen fixtures,
`bash scripts/check-forbidden-deps.sh`, formatting, and touched-crate clippy.

---

## SECTION 6 — SPRINT 3: positional `RecordValue` and consumer migration

- [ ] **3.1 ACTIVITY TODO — introduce positional records.** Implement
  `RecordValue { type_name: SharedString, fields: Box<[Value]> }` with checked
  construction/access/mutation/replacement and ordered iteration. Replace the
  map variant in `Value`. **Acceptance:** source scan finds no
  `HashMap<String, Value>` inside the runtime record representation; missing
  slots cannot panic; the 64-bit size gate from 0.3 is green.
- [ ] **3.2 ACTIVITY TODO — migrate interpreter field operations.** Use validated
  slots for member reads/writes, nested field/index updates, defaults, copies,
  equality, rendering/type tests, `LEN`, retain/release, weak traversal, and
  destructor paths. **Acceptance:** V3–V5 fixtures pass; runtime code does not
  call `get(name)`, `insert(name, ...)`, or hash a field name.
- [ ] **3.3 ACTIVITY TODO — migrate provider record projections.** Replace name
  maps in `bn_host_net` and `bn_lib_web` with typed constructors/projections
  whose slot constants derive from one declared layout contract rather than
  duplicated magic numbers. Other providers touched by compilation errors are
  migrated by the same rule. **Acceptance:** malformed provider record shape
  returns an actionable diagnostic, never indexes unchecked; host fixture
  behaviour is unchanged.
- [ ] **3.4 ACTIVITY TODO — add construction regression gate.** Add an `rg`-based,
  fail-closed repository check asserting: runtime records contain no string-key
  map; member/path instructions contain resolved field references; LLVM does
  not scan initialization instructions to derive layout; `bn_value` has no
  `bn_ir`/frontend dependency. Wire it beside the existing dependency/shared-
  core gates. **Acceptance:** deliberately reintroducing any forbidden shape
  makes the script fail.
- [ ] **3.5 ACTIVITY TODO — run cross-backend behaviour evidence.** Run the
  fixtures from 0.5 through interpret and every backend that claims support,
  including nested structs, inheritance, ownership, vector fields and error
  boundaries. **Acceptance:** GC-PAR is feature-scoped, not a single stdout
  smoke; unsupported target operations remain support diagnostics.

**Definition of Done:** `Value::Record` is positional, all consumers are
migrated, V1–V8 hold, and construction gates prevent restoration of name
hashing or inferred layout.

**Verification SPRINT 3 (timed):** `cargo test -p bn_value -p bn_interp`,
affected HOST/library crate tests, filtered runtime/CLI fixtures,
`python3 tests/test_compiler_parity.py`, the new construction gate,
forbidden-deps, formatting, and touched-crate clippy.

---

## SECTION 7 — SPRINT 4: performance, slow-test remediation, and close

- [ ] **4.1 ACTIVITY TODO — compare value-model performance.** Run the 0.6
  release-mode benchmark on the same machine/toolchain and record medians.
  Positional record read/write must be measurably faster than the retained
  HashMap baseline for 8 and 32 fields; shared-string clone must allocate zero
  and be faster than owned-string clone. If noise prevents a conclusion, raise
  sample count and record the uncertainty rather than claim a win.
- [ ] **4.2 ACTIVITY TODO — analyze every confirmed slow test.** Populate the
  table below for each individual test over 5.0 s. Prefer moving setup to a
  lower-level real integration seam, caching immutable build artifacts within
  the test process, reducing redundant fixture compilation, or splitting
  independent assertions. Preserve contracts and negative cases. Re-run three
  warm samples and record the median. **Acceptance:** each slow test is either
  improved with equivalent coverage or explicitly retained with a technical
  reason and cost.
- [ ] **4.3 ACTIVITY TODO — full regression and architecture gates.** Run the
  close battery in SECTION 9 with timings. Investigate any new failure or
  material runtime regression before proceeding. **Acceptance:** all required
  commands exit zero; no result is inferred or simulated.
- [ ] **4.4 ACTIVITY TODO — user-facing surface review.** Review root README,
  tutorial book, `docs/man/bn.1`, and `../basicnext-vscode`. Because this bucket
  changes no source surface, update only statements that describe architecture
  or performance incorrectly; record “reviewed, no change” where appropriate.
  Update the relevant release note and release index only after Carlos chooses
  the shipping version.
- [ ] **4.5 ACTIVITY TODO — close honestly.** Record final evidence and timings,
  update `AGENTS.md`/advisory status if requested by Carlos, move this file to
  `done/bucket-0.5.2.1b.md`, and repair links. **Acceptance:** release notes
  exist if a version is being closed; no open acceptance item is marked done;
  no commit/tag/push/release occurs without explicit instruction.

### 7.1 Slow-test analysis register

| ID | Test and layer | Before (cold/warm) | Cause | Coverage that must remain | Replan | After median | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **INF-01** | `cargo test --workspace` test-binary startup overhead (broad gate; not an individual test) | cold/incremental: 1067.03 s total; reported test bodies were 0.00–24.20 s per suite, with repeated 30–60 s gaps before small/empty binaries | Environment/toolchain process-start or artifact-validation overhead; compilation was 65 s and does not explain the repeated gaps | Entire workspace suite | Profile test-binary launch separately on a later warm run; do not rewrite individual tests until a test body itself exceeds 5 s | Not measured | Open infrastructure analysis; no individual >5 s test confirmed |

---

## SECTION 8 — Execution evidence and timing log

Append one row whenever a test command is actually run. Keep failed attempts;
they are evidence, not clutter.

| Date/commit | Sprint/activity | Layer | Command | Cache | Real time | Result | Follow-up |
| --- | --- | --- | --- | --- | ---: | --- | --- |
| 2026-09-19 / `f973498` | Pre-execution checkpoint | Broad | `cargo test --workspace` | cold/incremental; root test build 65 s | 1067.03 s | **FAIL:** 59/66 `bn_rt` tests passed; seven loopback bind tests failed with OS `PermissionDenied` (`Operation not permitted`) | Environment does not permit the loopback bind required by these real network tests. No mocks or skips added. Track startup overhead as INF-01 and rerun the failing crate in a network-capable environment before a green close claim. |

Performance evidence from 0.6/4.1 must additionally record CPU/OS, Rust
toolchain, release profile, sample count, field count, median ns/op or total
duration, allocation evidence, and before/after ratio.

---

## SECTION 9 — Required checks at close

Run each test-bearing command with SECTION 2 timing and record it in SECTION 8.
Build-only and static gates also record elapsed time when practical.

```sh
cargo build -p bn_rt
cargo build --workspace --all-targets
cargo fmt --check
cargo test -p bn_value
cargo test -p bn_ir
cargo test -p bn_frontend
cargo test -p bn_interp
cargo test -p bn_llvm
cargo test -p bn_lib_json
cargo test -p bn_host_net -p bn_lib_web
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-forbidden-deps.sh
bash tests/check-shared-cores.sh
# Run the new value/layout construction gate at its final repository path.
python3 tests/test_compiler_parity.py
python3 tests/test_wasm_parity.py
git diff --check
```

The SPRINT 3 gate activity must replace the descriptive construction-gate
comment above with its exact command before closure. If `rg` is missing, the
forbidden-dependency and construction scans fail closed; they are not skipped.

**Claim wording at close:** “Fragilidade 6 corrective scope (a)–(d) is closed:
field names/layouts are resolved once into validated BN IR; interpreted records
use positional boxed storage without per-access name hashing; immutable runtime
text is shared; and BNJson uses `serde_json::Value` behind the same bounded
contract. Functional, negative, cross-backend, representation and performance
evidence is recorded. Integer specialization and a public `Error` redesign
remain out of scope.”

Anything short of that remains **OPEN** or **PARTIAL**.

---

## SECTION 10 — Risks and mitigations

| Risk | Mitigation / acceptance evidence |
| --- | --- |
| Field slots drift across inheritance or imports | One lowered layout table; base-first invariant; negative validator cases; cross-module/inheritance fixtures on both backends. |
| Optimizing the interpreter silently changes LLVM layout | Both backends consume the same validated layout artifact; LLVM layout reconstruction is deleted and regression-gated. |
| `bn_value` acquires an upward dependency | Field identity stays in `bn_ir`; positional storage exposes checked numeric access only; forbidden-deps/construction gate checks Cargo and imports. |
| Positional access panics on malformed IR/provider data | `validate` rejects IR; runtime/provider projection remains checked and returns `INVALID_IR` or the existing domain diagnostic. |
| `Arc<str>` conversions allocate at every boundary | Central constructors plus source review; pointer-sharing/allocation evidence; conversions only at actual mutable/FFI boundaries. |
| Serde replacement accepts duplicate keys or changes resource bounds | Bounded adapter/custom visitor and exact boundary tests run before deletion of the private enum. |
| Broad migration becomes a god-module refactor | Sprints split text/JSON, IR, and runtime consumers; touched files observe the 500-line/local skill rule; unrelated renaming is excluded. |
| Performance test becomes flaky CI policy | Microbenchmark is ignored/release-mode evidence, not a wall-clock CI assertion; structural O(1)/no-hash gates are deterministic. |
| Test optimization reduces coverage | Every >5 s case records its owned contract and equivalence argument; no mock/skip/assertion weakening; functional suite must remain green. |
| Dirty worktree causes unrelated edits to be absorbed | Touch only named scope, inspect diffs by path, preserve all pre-existing user changes, and do not commit without instruction. |

---

## SECTION 11 — History

- **2026-09-19 — Opened.** Re-audited advisory Fragilidade 6 against
  `f973498`. Confirmed string-keyed runtime records, textual field references
  in IR, LLVM layout inference from initialization instructions, owned runtime
  strings, and the private BNJson value enum. Corrected the advisory's proposed
  identity: binding `SymbolId` must not be reused as a field id and `bn_value`
  must not depend on `bn_ir`. Planned four sequential implementation sprints
  after a contract/baseline sprint. Carlos added the execution rule that tests
  are timed when opportunely required by development; individual tests over
  5 s are analyzed and replanned for efficiency without sacrificing coverage.
  No implementation or Rust test run occurred while creating this bucket.
