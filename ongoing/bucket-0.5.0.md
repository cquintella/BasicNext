# Basic Next 0.5.0 — Corrective train (ARC DNA + typed dispatch returns)

**Status:** Active executable bucket (Tron audit 2026-09-12). **Tag policy locked by Carlos: ARC all-or-nothing on BOTH interpret (M2) and compile/LLVM (M4) + F1–F13 + typed AWAIT; not locks/dispatch-only; not interpret-only.**  
**Objective:** Make the **locked** 0.5.0 language DNA real: ARC compliance (no `DELETE`), typed `AWAIT → T OR Error`, fixtures F1–F13. Spec locks already exist; this file is the **implementation WBS**.

**Owner (tracker):** Tron until Carlos names implementer.  
**Gate:** Quorra ARC-compliance + no done-oco (fixtures + evidence) before Carlos.


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
| **M1** | Spec/grammar/book purge DELETE; WEAK; RELEASE; migration | **Mostly DONE** in `docs/0.5.0/` + book overlay; book ch.7 body still 0.3 manual text (superseded by overlay, not rewritten) | FE still parses `DELETE` (0.4 grammar live) |
| **M2** | Interpret strong/weak + F1–F13 | Spec ready | **NOT STARTED as language surface** — `bn_arc.rs` untracked private Rust helper only; heap still `USE_AFTER_DELETE` / `DOUBLE_DELETE` |
| **M3** | `value-memory-abi` object rows closed for interpret | Checklist still PARTIAL historically | **OPEN** |
| **M4** | LLVM retain/release + matrix | Deferred OK if interpret-first | **OPEN** |

### Track D — Typed dispatch

| Wave | Intent | Spec status | Code status (2026-09-12) |
| --- | --- | --- | --- |
| **D0** | Locks into language + BNDispatch module text | Spec in `docs/0.5.0` + proposal | **`modules/bn/BNDispatch.bn` still** `Async(work AS FUNCTION() AS VOID OR Error)` / Wait VOID — **not typed** |
| **D1** | FE + interpret ticket stores `Value`; AWAIT unboxes | Spec ready | **OPEN** (ABI `bn_rt_dispatch_await` already has result pointer) |
| **D2** | LLVM out-param + matrix | After D1 | **OPEN** |
| **D3** | `parallel_work` / `parallel_pi` honest | After D1 | **OPEN**; examples still DELETE-heavy elsewhere |

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
5. **M3 / D2 / D3 / M4** — as capacity; M4 may be 0.5.1.

## SECTION 1 — Sprint execution

Carlos confirmed on 2026-09-12: the execution unit is a **complete sprint**.
Tasks within a sprint are not separate delivery boundaries. The waves above
remain the scope authority; this section groups their executable work.

### SPRINT 1 — D0–D1 typed dispatch on interpret

- [ ] ACTIVITY TODO — Deliver typed worker arguments and results through
  BNDispatch, frontend analysis, validated IR, and interpret together.
  **Status:** Exploration complete; normative decision gate pending.
  **Objective:** Let callers aggregate actual worker results, including the
  existing F13 sum fixture, while preserving timeout/cancellation/task errors.
  **Dependencies:** D0 contract and the committed fixtures-first gate. F1–F13
  `program.bn` and `NOTES.md` are tracked in commit `98af828`.
  **Definition of Ready:** Existing 0.5.0 locks plus an explicit rule for
  awaiting a ticket whose worker result type cannot be established statically.
  **Decision gate D1-TYPE:** TODO: Carlos decides whether an unknown ticket
  result type causes a static diagnostic or uses the destination type with
  runtime verification. The current specification gives every ticket the
  same source-level `Dispatch.Ticket` type but requires `AWAIT` to return the
  originating worker's `T OR Error`; it does not define this boundary for
  opaque ticket parameters. Do not introduce generic syntax or silently
  default unknown payloads to VOID to resolve this gap.
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
  `cargo build --bin bn` passed; F13 currently fails `ASYNC_RETURN_TYPE`.

The sprint remains open at D1-TYPE. M1/M2 implementation does not start as a
substitute for completing this sprint. The release remains ARC all-or-nothing.

## Hard acceptance gate (no done-oco)

Do **not** move to `done/`, do not claim “0.5.0 closed”, and do not tag a language-complete 0.5.0 until:

1. **No `DELETE`** in 0.5.0 grammar path + new ARC fixtures (F12). Quorra rejects DELETE return.  
2. **F1–F13** have committed `.bn` (or harness) + evidence under `docs/superpowers/evidence/arc-0.5.0/` for every wave claiming ARC done.  
2b. Same fixtures **green on `bn build` / native** (M4) before 0.5.0 claim — interpret-only is insufficient.  
3. **Typed AWAIT** fixture green on interpret (D1) and on compile as required (D2).  
4. Rust gate green on the committed diff (`fmt` / `test` / `clippy` / `git diff --check`) with pasted evidence.  
5. Working-tree-only checkboxes / untracked `bn_arc` alone ≠ acceptance.

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

### Decision required before implementation

TODO: D1-TYPE in the bucket records the missing static typing rule for tickets
whose originating worker type is unknown, such as an opaque ticket parameter.
This is a language boundary decision, not permission to begin authorized work.
- 2026-09-12 — Carlos (via Quorra): **amplify** — ARC required on interpret **and** compile (M4 in claim); extra sprints at bucket end if needed.
