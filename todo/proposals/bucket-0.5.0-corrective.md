# Proposal: Bucket 0.5.0 — Corrective plan (memory ARC + typed dispatch returns)

**Status:** Proposed — **action plan / language DNA lock**, not implementation.  
**Date:** 2026-09-12  
**Owner (tracker):** Tron (ex-Doug) until Carlos names implementer.  
**Gate:** Quorra before Carlos.  
**Motivation:** Carlos authorized a corrective 0.5.0 plan because (1) object lifetime drifted from a **Swift-like ARC teaching model** toward manual `DELETE`/opaque handles, and (2) parallel tasks still cannot return values on the language surface even though `bn_rt` already carries them.

**Related:**
- Philosophy: [PHILOSOPHY.md](../../PHILOSOPHY.md)
- Memory today: [docs/book/en/07_memory_management.md](../../docs/book/en/07_memory_management.md), [docs/language/0.4/0.4.md](../../docs/language/0.4/0.4.md) (NEW/DELETE/DESTRUCTOR)
- ABI checklist (PARTIAL): [docs/architecture/value-memory-abi.md](../../docs/architecture/value-memory-abi.md)
- Runtime ARC stub: [crates/bn_runtime/src/bn_arc.rs](../../crates/bn_runtime/src/bn_arc.rs)
- Dispatch returns: [dispatch-typed-return.md](dispatch-typed-return.md) (updated by this train)
- Swift ARC overview (external reference Carlos cited): https://www.w3schools.com/swift/swift_memory_management.asp

Nothing here is normative until accepted into `docs/language/`, architecture contracts, and fixtures. **No code in this document.**

---

## Diagnosis — how we drifted

### A. Memory / lifetime

| Layer | What exists today | Evidence |
| --- | --- | --- |
| Language / book | **Manual** allocation: `NEW` / `DELETE` / `DESTRUCTOR`; aliases copy the handle without transferring ownership; `USE_AFTER_DELETE` / `DOUBLE_DELETE`; process exit may reclaim leaked memory **without** running destructors | `docs/book/en/07_memory_management.md`; `docs/language/0.4/0.4.md` Construction and destruction |
| Interpreter heap | Generation-tagged slots + `live`/`destroying` — not language-visible retain counts | `crates/bn_runtime/src/heap.rs` |
| Internal ARC | `BnArc<T>` = strong-only, read-only Rust `Arc` wrapper; tests only; **not** a BN surface | `crates/bn_runtime/src/bn_arc.rs`, `crates/bn_runtime/tests/bn_arc.rs` |
| Compiled / HOST objects | Opaque handles + explicit `*_close` / `DELETE` (File, DataFrame, net, tickets) | `value-memory-abi.md` release-slice rows; library host/bndata docs |
| Contract status | Identity/aliasing/DELETE/ABI ownership checklist still **PARTIAL / NOT CLOSED** | `value-memory-abi.md` header (2026-09-05) |

**Swift ARC (target teaching model, simplified):** strong references keep an object alive; assignment/parameter/return adjust strong counts; when strong count hits zero, deinit runs; **weak** breaks cycles (optional binding); **unowned** is a non-optional non-owning reference that assumes lifetime (advanced). Automatic for class instances; value types are copied.

**Desvio:** BN documents and teaches **manual DELETE** as the deterministic destruction point, while the runtime grew a **private** `BnArc` and more **handle/close** registries. That is not “Swift-like ARC for the User.” It is two incomplete stories (manual language + hidden RC) without one interpret↔compile contract.

### B. Parallel function returns

| Layer | Today | Evidence |
| --- | --- | --- |
| Language / BNDispatch | Task body `VOID OR Error`; `AWAIT` / Wait = completion only | `docs/language/0.3/bndispatch.md`; async-await 0.4 design slice |
| Ticket API | Status/Error/Close — **no** typed result | Module surface; proposal problem table |
| `bn_rt` | **`bn_rt_dispatch_await(ticket, timeout_ms, out_result: *mut BNValue, out_error)`** copies `state.result` | `crates/bn_rt/src/dispatch_abi.rs`; test `queue_submit_and_await_return_a_scalar_result` |
| ABI doc | BNValue result storage is caller-owned | `value-memory-abi.md` BNValue / Dispatch rows |

**Desvio:** runtime can return a value; language discards it. Aggregation examples cannot be honest (`parallel_work` / `parallel_pi`).

---

## Goals (0.5.0)

1. **Lock** a single language memory model for **class instances** that is **ARC-strong by default**, teachable, and compatible with PHILOSOPHY (explicit contracts, KISS, small core, made to teach).
2. Specify **weak** (required for cycles in the plan) and decide **unowned** (in or deferred).
3. Define the fate of **`DELETE` / Dispose** under ARC (deprecate, restrict, or redefine) so authors are not taught two conflicting truths.
4. Require **interpret = reference** for the new rules; list what LLVM must eventually match (no silent native drift).
5. **Accept and schedule** typed dispatch returns: primary surface **typed `AWAIT` → `T OR Error`**, wiring to existing `bn_rt_dispatch_await` result pointer.
6. Write acceptance criteria with **`.bn` fixtures + tests** (no “done” without evidence).

## Non-goals (0.5.0)

- Implementing LLVM retain/release insertion in this bucket’s *plan* phase (implementation may start after accept; plan must still name it).
- Tracing GC, full COW containers, changing struct **value** semantics.
- Making DataFrame/File/net handles into ARC classes in the same slice (keep HOST/registry closes unless a later proposal unifies vocabulary).
- Composite / DataFrame payloads on dispatch tickets (phase 2 after scalars).
- Shipping both typed `AWAIT` and `Ticket.Result()` as equal primary APIs.
- Closing unfinished 0.4.7 matrix compile gate by pretending ARC fixes FOR/PRINT gaps.

---

## Track A — Memory model (Swift-like ARC corrective)

### A.1 Target language rules (normative intent)

For **class instances** (reference types):

| Rule | Intent |
| --- | --- |
| Strong (default) | Locals, fields, and parameters to class types are strong unless annotated otherwise. Assign / pass / return increment; end of lifetime / reassignment decrement. |
| Zero strong | Run destructor chain (same order as today’s DELETE chain), then free. |
| Weak | Non-owning; does not keep alive; becomes empty/nil when object dies; used to break cycles. Surface: explicit annotation or type form TBD at accept (keep core small — prefer one spelling). |
| Unowned | **Deferred recommendation:** out of 0.5.0 MVP (easy to misuse; Swift advanced). Revisit after weak + cycles fixtures exist. |
| Cycles | Document that strong cycles leak until broken with weak; provide at least one teaching fixture. |

For **structs / scalars / vectors (value types):** unchanged copy semantics — not ARC.

### A.2 `DELETE` / Dispose under ARC

**Recommendation for 0.5.0 accept text (Carlos must lock one):**

| Option | Meaning | Verdict |
| --- | --- | --- |
| **R1 (preferred)** | Class `DELETE` becomes **unnecessary** for normal life; remaining `DELETE` on class instances is either removed from teaching or defined as “release this strong binding only” **without** forcing immediate deinit if other strong refs exist (Swift has no DELETE). Prefer **deprecate `DELETE` on classes** in book/spec once ARC lands. |
| R2 | Keep `DELETE` as **immediate dispose** ignoring other aliases | Reject — recreates use-after-delete footguns and fights ARC. |
| R3 | `DELETE` = assert unique strong owner then deinit | Possible escape hatch; only if teaching needs deterministic teardown — mark advanced. |

HOST handles (`FS.File`, DataFrame close, tickets) stay **explicit close + DELETE/close** until a separate proposal maps them into ARC types. Do not silently ARC-wrap every handle in 0.5.0.

### A.3 Interpret vs LLVM alignment

| Backend | 0.5.0 obligation |
| --- | --- |
| Interpret | Must implement the accepted ARC rules as **executable reference** (may use `BnArc` / counts internally). |
| LLVM / `bn_rt` | Must not claim ARC parity until support-matrix rows + retain/release strategy exist. Plan wave: document required inserts (retain on copy, release on end-of-life) and identity observables from `value-memory-abi.md` §1–2. |
| Conformance | Fixtures: aliasing, scope exit deinit, reassignment drop, weak zeroing, cycle+weak, no double-deinit. |

### A.4 Philosophy fit

- **Teachable:** one story — “classes are shared; strong keeps alive; weak breaks cycles.”
- **Explicit:** weak is annotated; value vs reference stays visible in types.
- **KISS:** no unowned in MVP; no GC.
- **Small core:** avoid new IR kinds until lowering needs them; prefer semantic + runtime first.
- **Anti-magic:** document retain points in the spec appendix for implementers; users see strong/weak, not hidden `BnArc`.

### A.5 Waves (memory)

| Wave | Work | Done when |
| --- | --- | --- |
| M0 | Accept this proposal’s A.* locks (strong default, weak yes, unowned deferred, DELETE policy R1 or R3) | Carlos/Quorra gate written into language decision log |
| M1 | Spec + book rewrite of ch.7; keyword/annotation grammar sketch; update PHILOSOPHY cross-links if needed | Docs PR accepted |
| M2 | Interpret implements strong/weak; fixtures green | Evidence under `docs/superpowers/evidence/` |
| M3 | Close `value-memory-abi` object lifetime rows for interpret | Checklist items marked with tests |
| M4 | LLVM retain/release design + matrix rows (may slip past 0.5.0 tag if interpret-first release) | Native parity on ARC subset or honest deferred |

---

## Track B — Typed dispatch returns

### B.1 Decision (lock for 0.5.0)

| Choice | Decision |
| --- | --- |
| Primary surface | **Typed `AWAIT ticket(ms) AS T OR Error`** when ticket comes from `FUNCTION() AS T OR Error` |
| Alternative | `Ticket.Result()` **not** primary; optional later compat only |
| ABI | Keep wiring to **`bn_rt_dispatch_await(..., BNValue *result, ...)`** — already true |
| Replay | **Replay** stored success value until `Close` (not consume-once) |
| MVP `T` | `VOID` \| `INTEGER` \| `FLOAT` \| `STRING` \| `BOOLEAN` |
| `Queue.Join` | Completion-only (no aggregated values) |

Details and acceptance fixtures: [dispatch-typed-return.md](dispatch-typed-return.md) (history updated to record these locks).

### B.2 Waves (dispatch)

| Wave | Work | Done when |
| --- | --- | --- |
| D0 | Accept locks above into language + BNDispatch | Spec/module text |
| D1 | Frontend typecheck + interpret ticket stores `Value`; await unboxes | Fixtures: workers return numbers; Start sums |
| D2 | LLVM uses existing ABI out-param; matrix after interpret | Support rows + parity |
| D3 | Rewrite `parallel_work` / `parallel_pi` docs/examples | Honest aggregation, no PRINT-as-API |

---

## Bucket shape / ordering

Suggested critical path for **0.5.0**:

1. **D0–D1** (typed await) — smaller, ABI ready, unlocks parallel honesty fast.  
2. **M0–M2** (ARC lock + interpret) — larger DNA; do not block D1.  
3. **M3 / D2** — contracts + native as capacity allows.  
4. **M4 / D3** — matrix + example promotion.

Shared release claim “0.5.0” requires: D1 green + M0 locked + M1 docs merged; M2 interpret ARC may be the same tag or immediately follow — **Carlos chooses** whether 0.5.0 ships ARC interpret or only locks+dispatch.

**Recommendation:** 0.5.0 ships **(a)** accepted ARC language lock + book draft, **(b)** typed await interpret+module, **(c)** evidence. Full ARC interpret + LLVM retain can be 0.5.0 vs 0.5.1 — prefer **interpret ARC in 0.5.0** if staffing allows; otherwise 0.5.0 = locks+dispatch, 0.5.1 = interpret ARC.

---

## Acceptance criteria (plan completion vs implementation)

### This proposal is “done” as a plan when

1. Quorra gate + Carlos accept the locks in A.2, A.5, B.1.  
2. File linked from `todo/proposals/README.md`.  
3. Opens below answered or explicitly deferred with Owner.

### Implementation later is “done” when (do not claim early)

**Dispatch:** criteria in `dispatch-typed-return.md` Acceptance section.  
**ARC interpret:**  
1. Teaching examples: strong share, scope deinit, weak cycle break.  
2. No reliance on `DELETE` for normal class lifetime in new examples.  
3. Conformance fixtures for aliasing/deinit on interpret.  
4. `value-memory-abi` object section updated with ARC observables.

---

## Risks

| Risk | Mitigation |
| --- | --- |
| Two memory stories (DELETE + ARC) confuse Users | Lock R1; rewrite book ch.7 in same PR as accept |
| Weak annotation grows syntax | One form only; no unowned in MVP |
| Native ships without retain and “looks fine” | Matrix gate; no ARC claim on compile until M4 |
| Dispatch dual API | Typed AWAIT only as primary |
| Scope collision with open 0.4.7 matrix | Keep 0.5.0 corrective separate from matrix compile sweep |

---

## Open questions (Carlos)

1. **DELETE policy:** R1 (deprecate class DELETE) vs R3 (assert-unique dispose)?  
2. **Weak spelling:** attribute vs type wrapper vs method — pick one for M1.  
3. **0.5.0 tag content:** locks+dispatch only, or include interpret ARC (M2)?  
4. **Unowned:** confirm deferred.  
5. **HOST handles:** confirm stay manual close in 0.5.0.  
6. Dispatch: confirm **replay until Close** and MVP type set including STRING.

---

## Decision summary

| Topic | 0.5.0 plan decision |
| --- | --- |
| Memory teaching model | Swift-like **strong default + weak for cycles** |
| Unowned | Deferred |
| DELETE on classes | Prefer deprecate under ARC (R1) — Carlos lock |
| BnArc | Implementation detail for interpret, not language API |
| DataFrame/File closes | Remain explicit HOST/registry for now |
| Dispatch primary | **Typed AWAIT → T OR Error** |
| Ticket.Result | Not primary |
| ABI | Existing `bn_rt_dispatch_await` result pointer |

---

## History

- 2026-09-12 — Drafted by Tron after Quorra brief; Carlos authorized corrective plan (docs only).
