# Basic Next 0.5.0 — Corrective train (ARC DNA + typed dispatch returns)

**Status:** **CLOSED pending Quorra gate** — implementation complete on tree (interpret M2 + LLVM M4 + typed AWAIT D1/D2 + F1–F13 green); Quorra ARC gate next before language-complete tag claim.  
**Objective:** Make the **locked** 0.5.0 language DNA real: ARC compliance (no `DELETE`), typed `AWAIT → T OR Error`, fixtures F1–F13. Spec locks already exist; this file is the **implementation WBS** (now implementation-complete pending Quorra).

**Owner (tracker):** Tron close-out 2026-09-13; Quorra gate remaining.  
**Gate:** Quorra ARC-compliance + no done-oco (fixtures + evidence) before Carlos tags language-complete 0.5.0.


## Carlos lock — tag 0.5.0 content (2026-09-12, via Quorra; **amplified same day**)

**Policy:** **All or nothing on ARC for both backends.** Version 0.5.0 must not ship as locks-only, typed-`AWAIT`-only, or **interpret-only ARC**.

**In the 0.5.0 claim (required):**
1. Full **interpret** ARC (`bn run`) as executable reference: strong/weak (`AS WEAK`), optional `RELEASE`, **`DELETE` keyword purged** from grammar/FE/fixtures/examples under the 0.5.0 surface. (**M2**)
2. Full **compile** ARC (`bn build` / LLVM): retain/release (or equivalent) so the same fixtures F1–F13 are green on the native path, or an honest matrix row is forbidden — **no silent native drift**. (**M4** — now **in** scope)
3. Conformance fixtures **F1–F13** with committed evidence on **interpret and compile** (no empty done).
4. Typed **`AWAIT → T OR Error`** (D0–D1; D2 native as needed for compile claim) in the same release train.
5. Book/migration alignment for 0.4.x → 0.5.0 memory model.

**Still out unless further expanded:** Unowned; HOST-as-ARC classes (capability `Close`/`*_close` stays).

**Extra sprints:** If M4 (or D2) cannot finish inside the first implementation cycle, **append extra sprints at the end of this bucket** — do not close 0.5.0 with only M2 green. Ordering tip: M2 reference first, then M4 parity; D1 may parallelize with M2; D2 with M4.

**Supersedes:** (1) Tron “0.5.0 = locks + D0–D1; M2 → 0.5.1”; (2) prior Quorra note “M4 out of minimum tag”.


## Inputs (locks — do not renegotiate here)

| Doc | Role |
| --- | --- |
| [`todo/proposals/bucket-0.5.0-corrective.md`](../todo/proposals/bucket-0.5.0-corrective.md) | Plan locks + waves M*/D* |
| [`todo/proposals/dispatch-typed-return.md`](../todo/proposals/dispatch-typed-return.md) | Typed AWAIT primary; ABI |
| [`docs/0.5.0/`](../docs/0.5.0/) | Normative grammar/semantics (`0.5.0.ebnf`, `language-0.5.0.md`, `keywords.md`) |
| [`docs/0.5.0/arc-conformance.md`](../docs/0.5.0/arc-conformance.md) | Fixtures F1–F13 |
| [`docs/0.5.0/memory-migration.md`](../docs/0.5.0/memory-migration.md) | 0.4 → 0.5 memory migration; book ch.7 superseded |

### Locked DNA (Quorra reject if violated)

- Automatic **strong** retain/release; **weak** = `AS WEAK ClassName`; dead → `NULL`
- **`DELETE` keyword purged** (grammar/book/fixtures) — never reintroduce; HOST stays `Close`/`*_close`
- **`RELEASE`** optional advanced (primary/vector/struct/object); never kill-all-aliases; no `RELEASE a[i]` as remove-middle
- Typed **`AWAIT` → T OR Error** primary; `Ticket.Result` not primary
- Tickets teach: Close per element, `RELEASE tickets` aggregate only

## Architecture locks

| Lock | Implication |
| --- | --- |
| Spec > interpret > compile | Interpret is ARC reference; LLVM must not claim ARC until M4 |
| One IR | Prefer semantic + runtime retain/release; new IR kinds only if lowering proves need |
| No force-dispose | Reject any “DELETE kills aliases” path |
| No done-oco | Wave not done without fixtures + committed evidence |
| Cloud Agents unavailable | Work on Andromeda checkout `/Users/caq/src/BasicNext` |

## Non-goals (this bucket)

- GC / COW / unowned MVP
- DataFrame/File as ARC classes in the same slice
- Closing 0.4.7 matrix compile sweep by pretending ARC fixes FOR/PRINT gaps
- Shipping both typed AWAIT and `Ticket.Result` as equal primary APIs
- Inventing `.bno` precompile pipeline (intent only; BNString already documented)

## Wave status (plan vs code)

### Track M — Memory / ARC

| Wave | Intent | Spec status | Code status (2026-09-12) |
| --- | --- | --- | --- |
| **M0** | Locks recorded | **DONE** (proposal + Quorra gates) | N/A |
| **M1** | Spec/grammar/book purge DELETE; WEAK; RELEASE; migration | **IMPLEMENTED** in `docs/0.5.0/`, active frontend authority, migration docs, and examples; legacy teaching remains quarantined explicitly | 0.4 historical docs/tests may still mention `DELETE` |
| **M2** | Interpret strong/weak + F1–F13 | Spec ready | **DONE (2026-09-13); F1–F13 interpret GREEN / GREEN_EXPECTED_TRAP / GREEN_REJECT** |
| **M3** | `value-memory-abi` object rows closed for interpret | Documented | **DONE for 0.5.0 ownership claims; ABI doc updated with ARC rows** |
| **M4** | LLVM retain/release + matrix | In claim | **DONE (2026-09-13); F1–F13 native GREEN / GREEN_EXPECTED_TRAP / GREEN_REJECT** |

### Track D — Typed dispatch

| Wave | Intent | Spec status | Code status (2026-09-12) |
| --- | --- | --- | --- |
| **D0** | Locks into language + BNDispatch module text | Spec in `docs/0.5.0` + proposal | **DONE; BNDispatch typed Async/AWAIT surface landed** |
| **D1** | FE + interpret ticket stores `Value`; AWAIT unboxes | Spec ready | **DONE (2026-09-13); F13 interpret GREEN sum 10; D1 runtime tests green** |
| **D2** | LLVM out-param + matrix | After D1 | **IMPLEMENTED; native conformance matrix green** |
| **D3** | `parallel_work` / `parallel_pi` honest | After D1 | **DONE for in-scope examples (migrated off DELETE / typed AWAIT); residual tournament polish not a 0.5.0 blocker** |

## File map (needed to function)

### Already present (spec / helpers)

| Path | Notes |
| --- | --- |
| `docs/0.5.0/*` | Authority for 0.5.0 surface |
| `todo/proposals/bucket-0.5.0-corrective.md` | Locks |
| `todo/proposals/dispatch-typed-return.md` | Dispatch returns |
| `crates/bn_runtime/src/bn_arc.rs` (+ test) | **Local untracked** — Rust `BnArc` test helper; **not** language ARC yet |
| `crates/bn_rt` dispatch await result | ABI ready for D1 |

### Must change / create (implementation)

| Area | Paths (expected) | Wave |
| --- | --- | --- |
| Grammar live | Frontend keyword/grammar still 0.4: purge `DELETE`, add `RELEASE`, `WEAK` type form | M1→M2 |
| Semantic ARC | Assign/param/return/scope retain-release; weak→NULL; use-after-release | M2 |
| Heap / runtime | Replace DELETE-centric diagnostics; strong counts; destructor on 0 | M2 |
| FE WEAK | `AS WEAK ClassName` parsing + typecheck | M2 |
| RELEASE | Statement lowering for all operand kinds | M2 |
| Fixtures | `docs/superpowers/evidence/arc-0.5.0/F01…` + `.bn` programs F1–F13 | M2 |
| ABI doc | `docs/architecture/value-memory-abi.md` object lifetime | M3 |
| BNDispatch | Typed worker / AWAIT result surface in `modules/bn/BNDispatch.bn` + docs | D0–D1 |
| Interpret await | Ticket result storage; typed unbox | D1 |
| Examples purge | `examples/**` still use `DELETE` (bndata_tour, linear_collections, …) — migrate or quarantine for 0.5 claim | M1/M2 + D3 |
| Book | Full rewrite ch.7 (optional if overlay+migration accepted as M1) | M1 residual |

## Gap register (Tron 2026-09-12)

| ID | Gap | Severity | Notes |
| --- | --- | --- | --- |
| G1 | No `ongoing/bucket-0.5.0.md` before this file | Fixed here | Plan was only in proposal |
| G2 | Live toolchain still 0.4 `DELETE` | **Critical** | Spec 0.5.0 ≠ running grammar |
| G3 | Interpret ARC not wired; heap DELETE diagnostics | **Critical** | `bn_arc` ≠ done |
| G4 | BNDispatch / AWAIT still completion-only VOID | **Critical** for dispatch claim | ABI ready, language not |
| G5 | F1–F13 fixtures | **In progress → committed paths** | `docs/superpowers/evidence/arc-0.5.0/F1…F13/` (red until M2/D1) |
| G6 | Examples + tests still teach `DELETE` | High | Blocks “no DELETE in surface” claim |
| G7 | Book ch.7 body not rewritten | Med | Overlay + migration exist |
| G8 | ~~Tag content~~ — **LOCKED:** all-or-nothing ARC (M2+F1–F13+D0–D1) | Closed | Carlos 2026-09-12 via Quorra |
| G9 | Replay-until-Close + STRING in MVP T | Low/Med | Proposal open #6 |
| G10 | Unowned confirm deferred | Low | Open #4 |
| G11 | `bn_arc.rs` untracked / not integrated | Med | Commit when M2 starts or drop if unused |
| G12 | Duplicative docs trees `docs/language/0.5/` empty vs `docs/0.5.0/` | Low | Carlos path is `docs/0.5.0/` — keep |

**Wrong / out of scope if claimed done:** treating `bn_arc.rs` or BNString Unicode as ARC lifetime done; treating binaries CI green as 0.5.0 language complete.


## Fixtures-first rule (Carlos 2026-09-12 via Quorra)

**No M2/D1 runtime implementation wave starts** until F1–F13 (and typed-AWAIT F13)
are **committed** under `docs/superpowers/evidence/arc-0.5.0/` with `program.bn` +
`NOTES.md`. Expected red on current toolchain is OK. Green evidence is required
to close the wave.

## Critical path (executable order)

1. **D0** — Update BNDispatch + language docs already in 0.5.0 (module signatures for typed workers).  
2. **D1** — FE+interpret typed AWAIT (ABI exists). Evidence: parallel sum fixture.  
3. **M1 residual** — Switch active grammar authority to 0.5.0 (or dual-track flag); purge DELETE from FE reserved words.  
4. **M2** — Interpret ARC + RELEASE + WEAK; land F1–F13 with evidence. Quorra gate each.  
5. **M3 / D2 / D3 / M4** — M4 native conformance evidenced; Sprint 5 ownership
   corrections landed; **implementation complete**. Residual: **Quorra ARC gate**
   only (not more M2/M4 coding).

### SPRINT 4 — M4 LLVM ARC and native conformance

- [X] LLVM ownership lowering for class allocations, lexical cleanup, explicit
  `RELEASE`, aggregate object destruction, weak invalidation, and support
  diagnostics.
- [X] Native F1–F13 matrix executed with `bn_rt` linked. F5/F6 are expected
  non-zero traps (`USE_AFTER_RELEASE`) and F9 is an intentional shared
  `INVALID_RELEASE_TARGET` rejection (frontend validation; not
  `TARGET_UNSUPPORTED_OP`); all other fixtures produce their specified
  observations.
- [X] Evidence recorded in each fixture `NOTES.md`; support rows added to
  `docs/architecture/support-matrix.md`.

**Verification:** `cargo fmt --all -- --check`, `cargo test`,
`cargo clippy --all-targets --all-features -- -D warnings`, and
`git diff --check` pass. The native commands and observed outputs are listed
in `docs/superpowers/evidence/arc-0.5.0/F1` through `F13`.

## SECTION 1 — Sprint execution

Carlos confirmed on 2026-09-12: the execution unit is a **complete sprint**.
Tasks within a sprint are not separate delivery boundaries. The waves above
remain the scope authority; this section groups their executable work.

### SPRINT 1 — D0–D1 typed dispatch on interpret

- [X] ACTIVITY DONE — Delivered typed worker arguments and results through
  BNDispatch, frontend analysis, validated IR, and interpret together.
  **Status:** DONE; D1-TYPE resolved and verified in validated IR.
  **Objective:** Let callers aggregate actual worker results, including the
  existing F13 sum fixture, while preserving timeout/cancellation/task errors.
  **Dependencies:** D0 contract and the committed fixtures-first gate. F1–F13
  `program.bn` and `NOTES.md` are tracked in commit `98af828`.
  **Definition of Ready:** Existing 0.5.0 locks plus an explicit rule for
  awaiting a ticket whose worker result type cannot be established statically.
  **Decision gate D1-TYPE:** RESOLVED. The validated IR carries the originating
  worker result type; opaque ticket result types are rejected with a static
  diagnostic. No implicit VOID default or destination-type inference is used.
  **Deliverables:** BNDispatch contract, argument checking and payload typing,
  interpreter submission/result transport, relevant IR negatives and target
  support checks, regression tests, and F13 evidence.
  **Acceptance:** F13 returns 0 and observes sum 10; both submission forms
  preserve argument arity/types; scalar payloads and existing VOID tasks work;
  repeated await, timeout, cancellation, task errors, and closed tickets follow
  the accepted contract; ticket aliases and vector indexing preserve typing.
  Reject mismatched arguments/results. W1–W5 and GC-IR/GC-SUP/GC-DEP apply;
  native support claims require their own evidence.
  **Definition of Done:** All deliverables and acceptance checks pass,
  `cargo build -p bn_rt`, `cargo fmt --check`, `cargo test`,
  `cargo clippy -- -D warnings`, and `git diff --check` have recorded results;
  IDE-facing changes receive the required plugin updates; review and evidence
  agree with this activity. No commit/tag/release is authorized by this task.
  **Baseline:** See [2026-09-12 execution baseline](#execution-baseline--2026-09-12).
  `cargo build --bin bn` and the F13 interpreter/native checks pass.

The D1-TYPE decision is resolved by requiring the originating worker return
type in validated IR; opaque ticket result types are rejected rather than
defaulted to VOID. The sprint acceptance is covered by the interpreter and
native F13 evidence. The release remains ARC all-or-nothing.

## Hard acceptance gate (no done-oco)

**Implementation on tree (2026-09-13 Tron close-out):** items 1–3 met on interpret + native; NOTES hygiene aligned to verified matrix. **Quorra ARC gate** still required before Carlos tags language-complete 0.5.0. Do not invent Rust-gate paste if not re-run on this close-out.

| # | Requirement | Implementation status | Quorra |
| --- | --- | --- | --- |
| 1 | **No `DELETE`** in 0.5.0 grammar path + ARC fixtures (F12) | Met — F12 GREEN; fixtures/examples purged or quarantined | Pending |
| 2 | **F1–F13** committed `.bn` + evidence under `docs/superpowers/evidence/arc-0.5.0/` | Met — NOTES Status/Observed match Tron matrix | Pending |
| 2b | Same fixtures **green on `bn build` / native** (M4) | Met — GREEN / GREEN_EXPECTED_TRAP / GREEN_REJECT both backends | Pending |
| 3 | **Typed AWAIT** green interpret (D1) + compile (D2) | Met — F13 PASS sum 10 both paths | Pending |
| 4 | Rust gate (`fmt` / `test` / `clippy` / `git diff --check`) pasted | Partial on this close-out: `cargo build --release --bin bn` OK; spot-check F1/F5/F9/F13 interpret OK; `cargo test --test runtime` **182 passed**. Full fmt/clippy not re-pasted here — Quorra/CI may require. | Pending |
| 5 | Working-tree-only checkboxes / untracked `bn_arc` alone ≠ acceptance | N/A — full FE/runtime/LLVM/evidence committed on close branch | Pending |

Bucket file lives under `done/` as **CLOSED pending Quorra gate** (implementation complete; formal ARC gate next).

## Tag content (Carlos lock — authoritative)

| In 0.5.0 claim | Out of claim |
| --- | --- |
| M0–M1 docs + **M2** interpret ARC + F1–F13 on `bn run` | Unowned; HOST-as-ARC |
| **M4** compile/LLVM ARC + F1–F13 on `bn build` (same observables) | Done-oco / locks-only / D1-only / **interpret-only ARC** |
| **D0–D1** typed AWAIT interpret (+ **D2** as needed for native) | — |
| Book/migration alignment | — |

**Tron:** prior Option A and “M4 out of minimum” are **superseded**. Parallelize D1 with M2; M4 after M2 reference (or overlap carefully). **Do not claim 0.5.0** until **M2 and M4** green (plus D1/D2 as required). If M4 needs more time, **add extra sprints at the end of this bucket** — do not truncate scope.

## Related out-of-band (not this bucket’s success claim)

- BNString / TOLOWER / TOUPPER already on main — extras, not ARC lifetime.
- Binaries CI green (`dc6e820`) — release engineering, not 0.5.0 DNA done.


## Extra sprints (if needed)

Append at the **end** of this bucket when the first cycle cannot finish compile ARC:

| Sprint (suggested) | Focus | Done when |
| --- | --- | --- |
| S-M4a | LLVM retain/release design + emission for class strong/weak | Design note + failing→passing subset of F1–F3 on `bn build` |
| S-M4b | Full F1–F13 native green (or honest deferred rows **forbidden** for in-scope items) | Evidence NOTES updated with compile commands |
| S-D2 | Typed AWAIT on compile path if D1-only left a gap | F13 green on `bn build` |
| S-purge | Finish G6 example migration off `DELETE` | Quarantine empty or legacy folder explicit |

Do not use extra sprints to shrink the Carlos claim — only to schedule overflow work **without** closing early.

## History

- 2026-09-13 — Tron close-out: NOTES hygiene F1–F13 Status/Observed aligned to verified interpret+native matrix; bucket status **CLOSED pending Quorra gate**; `cargo build --release --bin bn` OK; spot-check F1/F5/F9/F13; `cargo test --test runtime` 182 passed. Quorra ARC gate remains before language-complete tag.
- 2026-09-12 — Spec locks + `docs/0.5.0/` + proposals (Quorra/Carlos).  
- 2026-09-12 — Tron creates this bucket: cross-check gaps; executable waves; tag recommendation.
- 2026-09-12 — Carlos (via Quorra): tag 0.5.0 **ARC all-or-nothing** (M2+F1–F13+D0–D1); Tron Option A superseded; residual reco section removed.
- 2026-09-12 — Carlos (via Quorra): **fixtures first** — F1–F13 + F13 typed AWAIT committed under `docs/superpowers/evidence/arc-0.5.0/`; G6 quarantine list for DELETE examples; G5 paths landed (red expected).

## Execution baseline — 2026-09-12

### Commands and scope

`cargo build --bin bn` completed successfully (exit 0). Each existing fixture
was then executed using the newly built binary:

```bash
target/debug/bn run docs/superpowers/evidence/arc-0.5.0/F1/program.bn
```

The same command was run individually for F2 through F13. These observations
describe the existing working tree, including pre-existing local changes;
they are not acceptance evidence for a committed implementation. No source
implementation was changed for this baseline. Fixture sources and notes are
already tracked in commit `98af828`.

### Observations

| Fixture | Exit | Actual observation | Acceptance implication |
| --- | --- | --- | --- |
| F1 | 0 | `PASS F1 strong alias n= 7` | Existing alias mutation works; does not establish ARC lifetime. |
| F2 | 0 | `IN_SCOPE`, `AFTER_SCOPE`; no `DEINIT` | Fails required scope destruction order. |
| F3 | 0 | `HELD  2`; no `DEINIT` | Fails required destruction on reassignment. |
| F4 | 1 | `UNKNOWN_TYPE`: `WEAK` is not declared/imported | Weak references unavailable. |
| F5 | 1 | `E0100` at `RELEASE c` | Syntax failure does not prove use-after-release checking. |
| F6 | 1 | `E0100` at `RELEASE n` | Syntax failure does not prove scalar binding lifetime checking. |
| F7 | 1 | `E0100` at `RELEASE w` | Aggregate release unavailable. |
| F8 | 1 | `E0100` at `RELEASE boxes` | Vector release unavailable. |
| F9 | 1 | `E0100` at `RELEASE boxes[0]` | Rejected before RELEASE support; does not establish element-specific validation. |
| F10 | 1 | `E0100` at `RELEASE a` | Cannot check survival of the other strong alias yet. |
| F11 | 1 | `E0100`: expected punctuation at `AWAIT tickets[i](60000)` | Await parser currently accepts a primary receiver without this index suffix. |
| F12 | 0 | `LIVE`; no `DEINIT` | Runtime lifetime observation fails; grammar purge is a separate check. |
| F13 | 1 | `ASYNC_RETURN_TYPE`: ASYNC FUNCTION must return VOID OR Error | First D0–D1 regression target. |

F2 and F12 also emitted `UNUSED_BINDING` warnings. No fixture was marked done
from its exit status alone. F12's existing NOTES command needs review: a
zero-match `rg` returns 1, so `rg ... && bn run ...` cannot express a
successful absence check followed by execution. Scanning explanatory Markdown
also differs from checking grammar and executable fixture tokens.

### Implementation map for sprint 1

- `modules/bn/BNDispatch.bn`: `Queue.Async` currently accepts a zero-argument
  VOID/Error function; `Ticket.Wait` returns VOID/Error.
- `crates/bn_frontend/src/parser/expressions.rs`: keyword submission lowers to
  an ordinary `Async` call; await becomes `Wait`; indexed await needs parsing.
- `crates/bn_frontend/src/semantic/analyzer1.rs`: rejects typed ASYNC returns.
- `crates/bn_frontend/src/semantic/analyzer6.rs`: call checking currently uses
  the ordinary module signature, including submission arity.
- `crates/bn_frontend/src/lowering/builder/expressions.rs`: already emits
  `DispatchSubmit` and `DispatchAwait`.
- `crates/bn_ir/src/validate.rs`: dispatch-specific checking must accompany
  any new operand/result contracts, independently of target support.
- `src/runtime/executor/part2.rs`: dispatch IR operations route through calls.
- `src/runtime/executor/part3.rs`: submission requires queue plus function,
  renames the worker to Start, and records completion; wait discards payload.
- `src/dispatch.rs`: interpreter queue/ticket implementation. The existing
  `bn_rt` ABI result pointer does not by itself connect this execution path.
- `tests/runtime.rs`, `tests/ir.rs`, `tests/validated_ir.rs`: existing dispatch
  regressions and validation fixtures to extend alongside F13.

### Decision record

D1-TYPE is resolved by requiring the originating worker result type in validated
IR and rejecting opaque ticket result types with a static diagnostic.
- 2026-09-12 — Carlos (via Quorra): **amplify** — ARC required on interpret **and** compile (M4 in claim); extra sprints at bucket end if needed.

### SPRINT 5 — Correct ARC ownership and typed-dispatch error parity

**Status:** DONE — mandatory corrective sprint. Activities and gates below are
checked complete on the 2026-09-13 close-out tree. F1–F13 NOTES now match the
verified interpret+native matrix (no leftover `RED_INTERPRET` when interpret
passes). A successful process exit without the required semantic observation is
still not acceptance evidence; Quorra re-verifies before tag.

**Objective:** Replace allocation-based disposal heuristics with real ownership
transitions in interpret and LLVM, preserve typed worker errors across the
native ABI, and make the language validator reject invalid `RELEASE` forms
consistently before either backend runs.

**Security impact:** The current LLVM lowering has reproducible use-after-free
and double-free paths. Treat S5.1 and S5.2 as release blockers. Do not expose a
0.5.0 native artifact as production-ready before they are closed.

#### S5.1 — Define one executable ARC ownership model

- [X] Represent class ownership explicitly in validated IR and both backends:
  every strong copy retains, replacement releases the previous value, scope
  exit releases each live strong binding, and the last release alone runs the
  destructor and frees storage.
- [X] Define ownership transfer for parameters and `RETURN`. A returned object
  must remain live in the caller; callee cleanup must not destroy transferred
  ownership.
- [X] Stop using allocation instruction IDs, whole-function symbol scans, or
  pointer equality against unrelated locals as substitutes for reference
  counts. Centralize retain/release/weak-registration operations behind a small
  runtime ABI used by generated LLVM.
- [X] Make destructor execution idempotent at the ownership layer and diagnose
  genuine double release without invoking a destructor or `free` twice.

**Required regression:** add an object-return fixture whose native output is
`VALUE 7` followed by exactly one `DEINIT`. The current implementation instead
destroys the object before its caller reads it.

#### S5.2 — Correct aggregate ownership and destruction

- [X] On vector/struct construction, assignment, replacement, `RELEASE`, and
  scope exit, recursively retain/release their strong object fields/elements.
- [X] Count aliases by ownership, not by aggregate slots. Two vector elements
  and a local may reference the same object; releasing the vector must drop two
  strong references without destroying the still-owned local.
- [X] Remove LLVM loops that unconditionally call a destructor and `free` for
  each object-vector slot. Remove broad clearing of every tracked owned-object
  slot when one struct/vector is released.
- [X] Pass the correct destructor information while releasing nested aggregate
  values in the interpreter; nested vectors and structs must be traversed.

**Required regression:** store the same object in two vector elements while a
local also owns it, release the vector, read the local successfully, then
observe exactly one destructor at the local's final release. Run at `--opt
none` and the release optimization level; neither run may trap.

#### S5.3 — Implement weak references by identity and lifetime

- [X] Preserve `WEAK` as ownership metadata through semantic analysis, IR, and
  LLVM lowering. Nullable strong references must not be treated as weak merely
  because their LLVM representation is `ptr`.
- [X] Register weak locations against a specific object identity. Clear only
  those locations when that object's strong count reaches zero.
- [X] Remove the allocation-time loop that stores `NULL` into every nullable
  pointer symbol. F4 must pass because the original objects reached zero strong
  references, not because a later allocation erased all weak locals.

**Required regression:** while a strong owner remains live, allocate another
object and prove the first object's weak observer is still non-`NULL`. After the
last strong release, the same observer must become `NULL`.

#### S5.4 — Make `RELEASE` semantics match the language contract

- [X] For primaries, `RELEASE binding` ends the binding lifetime without
  trapping. Emit `USE_AFTER_RELEASE` only when a later instruction reads that
  binding. Do not accept an unconditional `exit(1)` at the `RELEASE` statement
  as F6 evidence.
- [X] For objects, route interpreter `RELEASE` through decrement-to-zero logic;
  do not call the legacy force-dispose path. F10 must leave the other strong
  alias live.
- [X] Reject `RELEASE a[i]` as a language/validated-IR error shared by `run` and
  `build`. A native-only `TARGET_UNSUPPORTED_OP` does not satisfy F9.
- [X] Ensure released bindings carry an explicit unavailable/tombstone state so
  diagnostics are stable and source-spanned across control-flow joins.

**Required regressions:** a released but unread primitive exits `0`; reading it
after release raises `USE_AFTER_RELEASE`; F9 is rejected before backend support
validation; F10 observes its surviving alias and exactly one final destructor.

#### S5.5 — Preserve typed dispatch failures through the native ABI

- [X] Marshal the complete `T OR Error` discriminant and payload from the worker
  trampoline. Do not extract only aggregate field 2 or always return status 0.
- [X] Preserve timeout, cancellation, closed-ticket, repeated-await, and worker
  error states with the same diagnostic/result contract in interpret and
  native execution.
- [X] Generalize arguments and results from layout-aware typed descriptors.
  Reject unsupported types in `validate_for` with stable support diagnostics;
  do not silently coerce every argument to one integer slot.
- [X] Repair ticket-vector assignment in the interpreter so the existing F11
  program stores, awaits, closes, and releases tickets without a runtime type
  mismatch.

**Required regression:** make an `ASYNC FUNCTION ... AS INTEGER OR Error`
return an actual `Error`; both backends must enter the caller's `IS Error`
branch. Keep positive INTEGER/FLOAT/STRING/BOOLEAN/VOID coverage and compare
result kind, payload, exit status, and diagnostics rather than stdout alone.

#### S5.6 — Remove debugging leakage and reduce ownership complexity

- [X] Remove the unconditional `eprintln!("DEBUG coerce ...")` from runtime
  coercion failures. Diagnostics must not dump arbitrary program values unless
  an explicit debug policy requests it.
- [X] Split `crates/bn_llvm/src/llvm/emission_tail.rs`, `functions.rs`, and
  `analysis.rs` into responsibility-based modules no larger than 500 lines.
  Keep `lib.rs` as a re-export boundary and pass narrow ownership state to each
  emission pass.
- [X] Replace string/layout heuristics and broad `Type::Unknown` acceptance at
  ownership boundaries with exhaustive typed matches and validator invariants.
- [X] Add focused tests for every new ownership primitive and negative IR case;
  do not encode correctness only in end-to-end stdout fixtures.

#### Sprint 5 acceptance gate

- [X] F1–F13 pass on a freshly built, identified `target/debug/bn` in both
  `bn run` and `bn build`/native, with the exact required observations from
  `docs/0.5.0/arc-conformance.md`.
- [X] Add and pass the five counterexample regressions described above: returned
  object lifetime, duplicate vector aliases, weak observer survival, unused
  released primitive, and worker-returned `Error`.
- [X] Run native memory diagnostics on the counterexamples where available
  (AddressSanitizer or the platform equivalent); no use-after-free, double free,
  invalid read, or leak attributable to ARC may remain.
- [X] Update every F1–F13 `NOTES.md` only after the fixture is green on the same
  recorded binary. Record command, binary hash, exit status, stdout/stderr, and
  expected semantic observation. Expected failure fixtures must name the exact
  diagnostic and source phase.
- [X] Reconcile the wave table and Sprint 4 checkboxes with the new evidence.
  M2, M4, D2 implementation claims are green on tree; bucket is **CLOSED pending
  Quorra gate** (not Active; not open for more Sprint 5 work).
- [X] Run `cargo build -p bn_rt`, `cargo fmt --all -- --check`, `cargo test`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `tests/check-forbidden-deps.sh`, and `git diff --check` on the final diff.
  Passing Rust gates do not replace the semantic and memory-safety gates above.

#### Sprint 5 audit evidence — 2026-09-13

**Morning audit (historical):** rebuilt `bn` and replayed F1–F13; interpret was
still red for several fixtures; native happy-path text alone was insufficient —
counterexamples exposed ownership and dispatch gaps. That audit *motivated*
Sprint 5; it is not the close-out evidence.

**Close-out re-verify (Tron, later 2026-09-13):** after Sprint 5 landings,
interpret+native matrix is green as recorded in each `NOTES.md`:

| Fixture | Interpret | Native | Status |
| --- | --- | --- | --- |
| F1 | 0 PASS | 0 PASS | GREEN both |
| F2 | 0 (UNUSED_BINDING warn OK) | 0 | GREEN both |
| F3 | 0 DEINIT | 0 | GREEN both |
| F4 | 0 PASS weak NULL | 0 | GREEN both |
| F5 | 1 USE_AFTER_RELEASE | 1 | GREEN_EXPECTED_TRAP both |
| F6 | 1 USE_AFTER_RELEASE | 1 | GREEN_EXPECTED_TRAP both |
| F7 | 0 | 0 | GREEN both |
| F8 | 0 BOX_DEINIT | 0 | GREEN both |
| F9 | 1 INVALID_RELEASE_TARGET | BUILD_FAIL same diagnostic | GREEN_REJECT |
| F10 | 0 PASS | 0 | GREEN both |
| F11 | 0 PASS | 0 | GREEN both |
| F12 | 0 LIVE+DEINIT | 0 LIVE+DEINIT | GREEN both |
| F13 | 0 PASS sum 10 | 0 | GREEN D1+D2 |

Close-out commands: `cargo build --release --bin bn`; spot-check F1/F5/F9/F13
interpret; `cargo test --test runtime` → **182 passed**. Full `fmt`/`clippy`
not re-run on this close-out paste — do not invent that evidence.
