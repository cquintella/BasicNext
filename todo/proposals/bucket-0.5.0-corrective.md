# Proposal: Bucket 0.5.0 — Corrective plan (memory ARC + typed dispatch returns)

**Status:** Proposed — **action plan / language DNA lock**, not implementation. **Carlos locks 2026-09-12 (via Quorra):** ARC compliance; **`DELETE` keyword removed from language DNA**; **`RELEASE` = optional advanced in MVP** (drop one strong only).  
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
3. **Locked:** **`DELETE` is removed from the BN language surface** (not merely deprecated on classes — keyword gone from DNA 0.5.0). Lifetime = ARC strong/weak + end of scope; **`RELEASE`** remains optional advanced (drop one strong only). HOST closes stay as capability methods (`Close` / `*_close`), never reintroduced as `DELETE`.
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
- **Keeping or reintroducing the `DELETE` keyword** on any BN surface (classes, teaching, or HOST “disguised as DELETE”).
- **Force-dispose / “kill all aliases”** for class instances (anti-ARC; rejected).
- Teaching semi-manual lifetime for classes (User must not need to sprinkle dispose to be correct).
- Implementing parser/FE removal of `DELETE` in this plan document (docs lock only; code later).

---

## Track A — Memory model (Swift-like ARC corrective)

### A.1 Target language rules (normative intent)

**Objective (Carlos lock):** BN **must be able** to implement ARC, and the language/toolchain **must require ARC compliance** — not a semi-manual story where the User is responsible for dispose.

For **class instances** (reference types):

| Rule | Intent |
| --- | --- |
| Strong (default, automatic) | Locals, fields, params, and returns of class types are strong unless annotated weak. **Assign / param / return / scope** insert retain/release in the toolchain (interpret reference; LLVM later). User does not manually retain. |
| Zero strong | Run destructor chain, then free. |
| Weak | Non-owning; breaks cycles; becomes empty/nil when object dies. Spelling TBD (open). |
| Unowned | **Deferred** (out of MVP). |
| Cycles | Strong cycles leak until broken with weak; at least one teaching fixture. |
| No force-dispose | There is **no** operation that destroys a live object while other strong aliases remain. |

For **structs / scalars / vectors (value types):** unchanged copy semantics — not ARC.

### A.2 `DELETE` removed; `RELEASE` optional — **LOCKED (Carlos via Quorra, 2026-09-12)**

| Lock | Decision |
| --- | --- |
| **`DELETE` keyword** | **Removed from language DNA for 0.5.0** — not “deprecate on classes only.” No `DELETE` in grammar, book teaching, or fixtures for the new model. Migration note required for 0.4.x programs that used `DELETE`. |
| Force-immediate destroy | **Out** with the keyword (former R2/R3 rejected). |
| Normal lifetime | ARC **strong/weak** + **end of scope / reassignment**. Hello needs no dispose keyword. |
| `RELEASE` (locked) | Optional **advanced** MVP: early end of binding for **primaries, fixed vectors, structs, and class objects**. Class = drop one strong (deinit at count→0; never kill-all-aliases). Vector/struct = end aggregate (nested rules). Primary = end local only (no RC). Use-after-release = error. Fixed vector: whole aggregate OK; **not** `RELEASE a[i]` as remove-middle. Tickets: Close per element, then optional `RELEASE tickets`. |
| HOST / registry | Capability methods stay (`Close`, `*_close`, ticket close). **Do not** reintroduce `DELETE` as sugar over those closes. Unifying HOST into ARC types is a **later** proposal. |
| Former options | Superseded: R1 “deprecate class DELETE” → escalated to **full keyword removal**. R2/R3 rejected. |

**Docs/work implied (still plan, not FE):** purge `DELETE` from book ch.7, language grammar/spec, and ARC fixtures; add a short **0.4.x → 0.5.0 migration** note (replace `DELETE x` with scope end / `RELEASE` where intentional early drop; HOST keeps `Close`).

### A.3 Interpret vs LLVM alignment

| Backend | 0.5.0 obligation |
| --- | --- |
| Interpret | Must implement the accepted ARC rules as **executable reference** (may use `BnArc` / counts internally). |
| LLVM / `bn_rt` | Must not claim ARC parity until support-matrix rows + retain/release strategy exist. Plan wave: document required inserts (retain on copy, release on end-of-life) and identity observables from `value-memory-abi.md` §1–2. |
| Conformance | Fixtures **must prove ARC compliance**: aliasing, scope deinit, reassign drop, weak cycle; **no `DELETE` in surface**; `RELEASE` never kills all aliases. No double-deinit. |

### A.4 Philosophy fit

- **Teachable:** one story — “classes are shared; strong keeps alive; weak breaks cycles.”
- **Explicit:** weak is annotated; value vs reference stays visible in types.
- **KISS:** no unowned in MVP; no GC.
- **Small core:** avoid new IR kinds until lowering needs them; prefer semantic + runtime first.
- **Anti-magic:** document retain points in the spec appendix for implementers; users see strong/weak, not hidden `BnArc`.

### A.5 Waves (memory)

| Wave | Work | Done when |
| --- | --- | --- |
| M0 | Locks recorded: automatic strong, weak for cycles, **`DELETE` keyword removed**, `RELEASE` optional-advanced in MVP, ARC compliance | Carlos locks (done 2026-09-12 via Quorra) + decision log pointer |
| M1 | Spec + book ch.7 rewrite; **purge `DELETE`** from grammar/teaching/fixtures; migration note 0.4.x; weak spelling; document `RELEASE` advanced | Docs PR accepted; **Quorra ARC-compliance gate** |
| M2 | Interpret implements strong/weak retain/release; compliance fixtures green | Evidence under `docs/superpowers/evidence/`; **Quorra ARC gate** |
| M3 | Close `value-memory-abi` object lifetime rows for interpret | Checklist + tests; **Quorra ARC gate** |
| M4 | LLVM retain/release design + matrix rows (may slip past 0.5.0 tag if interpret-first) | Native parity or honest deferred; **Quorra ARC gate** |

**Quorra’s role:** ARC **compliance gate** on every M* wave — reject any wave that reintroduces `DELETE`, semi-manual dispose, or force-destroy semantics.

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

1. Quorra gate + Carlos accept the locks in A.2 (DONE: `DELETE` removed; `RELEASE` optional advanced), A.5, B.1; remaining opens listed below.  
2. File linked from `todo/proposals/README.md`.  
3. Opens below answered or explicitly deferred with Owner.

### Implementation later is “done” when (do not claim early)

**Dispatch:** criteria in `dispatch-typed-return.md` Acceptance section.  
**ARC interpret:**  
1. Teaching examples: strong share, scope deinit, reassign drop, weak cycle break — **no `DELETE`**.  
2. `RELEASE` only as optional advanced drop-one-strong (hello without it).  
3. Compliance fixtures: aliasing, scope deinit, reassign drop, weak cycle; grammar/fixtures have **zero** `DELETE`; `RELEASE` never kill-all-aliases.  
4. Book ch.7 + migration note for 0.4.x; HOST closes remain `Close`/`*_close`.  
5. `value-memory-abi` object section updated with ARC observables.  
6. Quorra ARC-compliance gate passed for the wave claiming done.

---

## Risks

| Risk | Mitigation |
| --- | --- |
| `DELETE` lingering in book/grammar vs ARC story | **Purge keyword**; migration note; Quorra gate each M* |
| Weak annotation grows syntax | One form only; no unowned in MVP |
| Native ships without retain and “looks fine” | Matrix gate; no ARC claim on compile until M4 |
| Dispatch dual API | Typed AWAIT only as primary |
| Scope collision with open 0.4.7 matrix | Keep 0.5.0 corrective separate from matrix compile sweep |

---

## Open questions (Carlos)

1. ~~**DELETE policy**~~ — **LOCKED:** **`DELETE` keyword removed from language DNA** (not deprecate-only).  
2. **Weak spelling:** attribute vs type wrapper vs method — pick one for M1.  
3. **0.5.0 tag content:** locks+dispatch only, or include interpret ARC (M2)?  
4. **Unowned:** confirm deferred.  
5. ~~**HOST handles**~~ — **LOCKED for 0.5.0:** stay capability `Close` / `*_close` (not ARC-wrapped; **not** reintroduced as `DELETE`). Later unify proposal optional.  
6. Dispatch: confirm **replay until Close** and MVP type set including STRING.  
7. ~~**`RELEASE` in MVP?**~~ — **LOCKED:** optional advanced; applies to primaries/vector/struct/object (early binding end); class = drop one strong; use-after-release error; hello may omit.

---

## Decision summary

| Topic | 0.5.0 plan decision |
| --- | --- |
| Objective | BN **can** implement ARC; surface/toolchain **require ARC compliance** (not semi-manual) |
| Memory teaching model | Swift-like **automatic strong + weak for cycles** + end of scope |
| Strong | Toolchain/interpret inserts retain/release on assign/param/return/scope; zero strong → destructor |
| Unowned | Deferred |
| **`DELETE` keyword** | **Removed from language DNA** (grammar/book/fixtures) — Carlos lock 2026-09-12 via Quorra |
| `RELEASE` | **In MVP as optional advanced** — drop one strong only; deinit only at count→0; hello may omit; never kill-all-aliases |
| Tickets teaching (Carlos) | `tickets[i].Close()` per element (bn_rt); `RELEASE tickets` only on the **aggregate**; **never** `RELEASE tickets[i]`; hello may omit RELEASE and leave scope |
| `RELEASE` operand kinds | Primaries, fixed vectors, structs, class objects — early end of binding; use-after-release error |
| BnArc | Implementation detail for interpret, not language API |
| HOST / DataFrame / File | Capability **`Close` / `*_close` only** — do not reintroduce `DELETE` |
| Migration | Book/spec note for 0.4.x programs that used `DELETE` |
| Quorra | ARC **compliance gate** on every M* wave (reject `DELETE` return) |
| Dispatch primary | **Typed AWAIT → T OR Error** |
| Ticket.Result | Not primary |
| ABI | Existing `bn_rt_dispatch_await` result pointer |

---

## History

- 2026-09-12 — Drafted by Tron after Quorra brief; Carlos authorized corrective plan (docs only).
- 2026-09-12 — Carlos lock (via Quorra): ARC compliance required; automatic strong; weak for cycles; class force-`DELETE` out; remnant drop-one-strong = `RELEASE` or omit MVP; Quorra gates each M*; fixtures must prove no “kill all aliases.” Plan only — no runtime yet.
- 2026-09-12 — Carlos final lock (via Quorra): `RELEASE` **in MVP as optional advanced** (not omitted); semantics = drop one strong only; deinit only count→0; never kill-all-aliases.
- 2026-09-12 — Carlos lock (via Quorra): **remove `DELETE` from the language** (full DNA purge, not class-only deprecation); HOST stays `Close`/`*_close`; migration note for 0.4.x; plan/docs only — no parser yet.
- 2026-09-12 — Carlos teaching lock (via Quorra): tickets — Close per element (bn_rt); RELEASE only on aggregate `tickets`; never `RELEASE tickets[i]`; hello may omit RELEASE.
- 2026-09-12 — Carlos lock (via Quorra): RELEASE applies to primaries/vector/struct/object; use-after-release error; no RELEASE a[i] as remove-middle.
