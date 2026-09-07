# Proposal: Typed Dispatch task results (language + module surface)

**Status:** Proposed — **language DNA + BNDispatch module contract** (not examples-only).  
**Date:** 2026-09-07  
**Owner (tracker):** Doug until Carlos names implementer.  
**Motivation:** Without a way to **capture worker return values**, `BNDispatch` /
`ASYNC`/`AWAIT` only synchronize side effects (`PRINT`). Aggregation examples
(`parallel_work`, `parallel_pi`) cannot honestly sum results. Carlos: *se não
dá para pegar o retorno, não faz sentido ter.*

**Related (do not conflate):**
- [`docs/language/0.3/bndispatch.md`](../../docs/language/0.3/bndispatch.md) — current 0.3 VOID ticket surface
- [`docs/superpowers/specs/2026-09-01-async-await-0.4-design.md`](../../docs/superpowers/specs/2026-09-01-async-await-0.4-design.md) — locked initial slice: `VOID OR Error`
- [`docs/superpowers/specs/2026-09-05-bndispatch-abi-design.md`](../../docs/superpowers/specs/2026-09-05-bndispatch-abi-design.md) — **`bn_rt_dispatch_await(..., BNValue *result, ...)` already exists**
- Module today: `Queue.Async(FUNCTION() AS VOID OR Error)`; `Ticket` has no `Result`/`Value`

Nothing here is normative until accepted into `docs/language/` + `modules/bn/BNDispatch.bn`.

---

## Problem

| Layer | Today |
| --- | --- |
| Language / module | `Async` / `ASYNC FUNCTION` → task body **`VOID OR Error` only**; `AWAIT ticket(ms)` → **`VOID OR Error`** (completion/timeout/cancel), **not** payload |
| Ticket API | `Wait` / `Status` / `Error` / `IsDone` / `Close` — **no** typed result accessor |
| Examples | Workers `PRINT`; `parallel-examples.md` documents the gap |
| `bn_rt` ABI | `bn_rt_dispatch_await` already takes `BNValue *result` and stores task result in ticket state |

So the **runtime boundary can carry a value**; the **language surface discards it**. That is the bug/product gap — not missing PRINT.

Evidence: `bn check examples/parallel_work.bn` rejects `WorkA() AS INTEGER` with `TYPE_MISMATCH` against `FUNCTION() AS VOID OR Error`.

---

## Goals

1. Allow named tasks to return a **typed success value** `T` (at least scalar/`FLOAT`/`INTEGER`/`STRING`/`BOOLEAN` in MVP; `Error` still via `OR Error`).
2. Let `Start` (or any awaiter) **bind** that value after wait — so sum/reduce examples are honest.
3. Keep one IR story: `DispatchSubmit` / `DispatchAwait` already in ir-contract; extend typing, do **not** fork a second dispatcher.
4. Preserve fail-closed timeouts/cancel; do not invent shared mutable globals for aggregation.
5. Interpret = reference; compile claims only via support matrix after interpret fixtures exist.

## Non-goals (MVP)

- Passing arbitrary object graphs / DataFrames as task results (phase 2).
- Changing queue bounds, worker isolation, or panic→FAILED ticket policy.
- Making `PRINT` from workers the aggregation API.
- Stubbing examples with fake `Ticket.Result` before the module/FE accept.

---

## Proposed surface (preferred)

### 1. Widen submit signature (module + semantic)

```basic
' Today
queue.Async(work AS FUNCTION() AS VOID OR Error) AS Ticket OR Error

' Proposed MVP — result-bearing tasks
queue.Async(work AS FUNCTION() AS T OR Error) AS Ticket OR Error
' T in MVP: INTEGER | FLOAT | STRING | BOOLEAN | VOID
' (VOID keeps current programs valid)
```

Same for `ASYNC queue Name()` / `ASYNC FUNCTION` forms in 0.4 once grammar matches.

### 2. Await produces the value

**Preferred (matches user mental model):**

```basic
LET part AS FLOAT OR Error = AWAIT ticket(60000)
IF part IS Error THEN
    PRINT part.Code
ELSE
    total += part
END IF
```

Typing rule: if the ticket was created from `FUNCTION() AS T OR Error`, then
`AWAIT ticket(ms)` has static type **`T OR Error`** (timeout/cancel/task failure
still `Error`). Awaiting a `VOID` task stays `VOID OR Error`.

**Alternative (method form, if AWAIT must stay VOID for compatibility):**

```basic
LET done AS VOID OR Error = ticket.Wait(60000)
LET part AS FLOAT OR Error = ticket.Result()   ' Error if not successfully completed
```

Recommend **one** primary form in the accept text (prefer typed `AWAIT`); avoid shipping both without a deprecation story.

### 3. Ticket still opaque for scheduling

`Id` / `Status` / `Cancel` / `Close` unchanged. Result is available only after
successful completion; second await may return the same stored value (align with
async-await design “await more than once after completion”) **or** consume-once —
**lock one** at accept (recommend: **replay stored success value** until `Close`).

---

## Implementation sketch (honest layers)

| Layer | Work |
| --- | --- |
| Spec | Amend 0.3/0.4 dispatch + async-await: result-bearing tasks; typed await |
| `BNDispatch.bn` | Signatures for `Async` / `Wait`/`Result` or document typed `AWAIT` only |
| Frontend | Typecheck function-pointer arity/result; lower `DispatchAwait` destination type = `T` |
| Interpreter | On task finish, box return `Value` into ticket; await unboxes to caller |
| `bn_rt` | **Already** has `result` out-param — wire language path to it; extend `BNValueKind` only as needed for MVP types |
| LLVM | Same ABI; matrix rows only after interpret fixtures |
| Examples | Then rewrite `parallel_work` / `parallel_pi` to sum returns; update `parallel-examples.md` |

**Stop-the-line:** do not mark examples Fixed until interpret fixtures green.

---

## Acceptance (when this proposal is scheduled)

1. Normative language + library text accepted.
2. Fixture: four workers return INTEGER/FLOAT; `Start` awaits each; prints **one** sum; `bn run` exit 0; no reliance on PRINT order for the sum.
3. `FUNCTION() AS VOID OR Error` tasks still compile and await as today.
4. Timeout / cancel / task `Error` → awaiter sees `Error`, not a partial fake `T`.
5. Evidence note under `docs/superpowers/evidence/`.

---

## Bucket placement

- **Not** a silent add to 0.4.6 (closed security).
- Candidate: activity under **0.4.7** (after GP0) **or** dedicated DNA bucket / AQ — Carlos chooses. Matrix LLVM claim for dispatch returns is **after** interpret.

## Open questions (must close at accept)

1. Typed `AWAIT` vs `Ticket.Result()` as primary?
2. Result replay vs consume-once?
3. MVP type set: include `STRING`? exclude composite?
4. Does `Queue.Join` stay completion-only (yes recommended) while per-ticket await carries values?

---

## History

- 2026-09-07 — Drafted after parallel-example request blocked on VOID-only surface; ABI already has result pointer.
