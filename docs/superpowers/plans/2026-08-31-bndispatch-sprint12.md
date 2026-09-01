# BNDispatch Sprint 12 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the bounded `BNDispatch` queue and ticket foundation for the 0.3 Sprint 12 backport.

**Architecture:** `BNDispatch` is a standard external module recognized by the module graph and semantic model. Its runtime provider owns bounded queue state; workers execute immutable IR function definitions through isolated executors and communicate only completion state back to the submitting executor.

**Tech Stack:** Rust standard library (`std::thread`, `Mutex`, `Condvar`, `mpsc`), Basic Next handwritten frontend, interpreter runtime, Rust integration tests.

**Spec:** `docs/superpowers/specs/2026-08-31-bndispatch-sprint12-design.md`

## Global Constraints

- `BNDispatch` requires an explicit import and is not a `HOST` capability.
- Workers are bounded to 1..64 and pending tickets to 1,024 per queue.
- `Queue.Auto()` uses `HOST.NumProcs()` and clamps to 64.
- Tasks are named `FUNCTION() AS VOID OR Error`; closures, captures, task arguments, generic results, and OS thread handles are excluded.
- Every wait accepts 1..60,000 milliseconds and has deterministic close/failure behavior.
- No `unsafe`, no new dependency, no Internet-dependent verification, and no commit.

---

### Task 1: Register the external BNDispatch module

**Files:**
- Create: `modules/bn/BNDispatch.bn`
- Modify: `src/module_graph.rs`
- Modify: `src/semantic.rs`
- Test: `tests/modules/bndispatch-identities/main.bn`
- Test: `tests/module_graph.rs`

**Interfaces:**
- Produces `StandardModule::BNDispatch` and imported `Dispatch` namespace.
- Consumes standard-module import resolution and provider identity checks.

- [ ] **Step 1: Write failing module-resolution and semantic fixtures**

```basic
IMPORT BNDispatch AS Dispatch
FUNCTION Start() AS VOID
    LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
END FUNCTION
```

- [ ] **Step 2: Run the focused module tests and verify `BNDispatch` is unresolved**

Run: `cargo test --test module_graph bndispatch -- --nocapture`

- [ ] **Step 3: Add the module declaration and standard-module identity**

Create `Queue` and `Ticket` declarations with the signatures in the design;
add `StandardModule::BNDispatch` and route `BNDispatch.bn` through the existing
standard-module path.

- [ ] **Step 4: Run the focused module tests and verify import resolution**

Run: `cargo test --test module_graph bndispatch -- --nocapture`

### Task 2: Add queue and ticket runtime values

**Files:**
- Create: `src/dispatch.rs`
- Modify: `src/runtime.rs`
- Test: `tests/runtime.rs`

**Interfaces:**
- Produces opaque `Value::DispatchQueue` and `Value::DispatchTicket` provider values.
- Consumes named no-argument function values and runtime `Error` representation.

- [ ] **Step 1: Write failing runtime tests for queue construction and ticket states**

```basic
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
LET ticket AS Dispatch.Ticket OR Error = queue.Async(Work)
PRINT ticket.Status() = Dispatch.PENDING
```

- [ ] **Step 2: Run the focused runtime test and verify the provider is unavailable**

Run: `cargo test --test runtime dispatch_queue -- --nocapture`

- [ ] **Step 3: Implement bounded queue/ticket state and constant dispatch**

Use a queue-owned mutex and condition variable; validate worker and pending
limits before allocating a ticket; represent completion with the five accepted
numeric states.

- [ ] **Step 4: Run focused tests for state, saturation, and invalid limits**

Run: `cargo test --test runtime dispatch_queue -- --nocapture`

### Task 3: Execute isolated named functions and implement ticket lifecycle

**Files:**
- Modify: `src/runtime.rs`
- Modify: `src/dispatch.rs`
- Test: `tests/runtime.rs`

**Interfaces:**
- Consumes `Queue.Async(FUNCTION() AS VOID OR Error)` and queue/ticket state.
- Produces `Ticket.Wait`, `Ticket.Cancel`, `Ticket.Error`, `Ticket.IsDone`, and `Ticket.Close`.

- [ ] **Step 1: Write failing runtime tests for serial execution, concurrent capacity, task failure, cancellation, and close**

```basic
LET completed AS VOID OR Error = ticket.Wait(1000)
IF completed IS Error THEN
    PRINT completed.Message
END IF
```

- [ ] **Step 2: Run the focused runtime tests and verify each lifecycle behavior fails**

Run: `cargo test --test runtime dispatch_ticket -- --nocapture`

- [ ] **Step 3: Create isolated worker executors and wire ticket completion**

Workers invoke only declared no-argument functions, capture `Flow::Return` and
runtime diagnostics as ticket completion, and never share a submitting
executor's heaps, statics, native handles, input, or output.

- [ ] **Step 4: Run lifecycle tests and verify serial/concurrent behavior**

Run: `cargo test --test runtime dispatch_ticket -- --nocapture`

### Task 4: Add queue join, close, and automatic worker selection

**Files:**
- Modify: `modules/bn/BNDispatch.bn`
- Modify: `src/runtime.rs`
- Modify: `src/dispatch.rs`
- Test: `tests/runtime.rs`

**Interfaces:**
- Consumes tickets and `HOST.NumProcs()`.
- Produces `Queue.Auto`, `Queue.Join`, and `Queue.Close`.

- [ ] **Step 1: Write failing tests for Auto clamping, join timeout, pending cancellation, and close idempotence**

```basic
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Auto()
LET closed AS VOID OR Error = queue.Close(1000)
```

- [ ] **Step 2: Run focused close/auto tests and verify they fail**

Run: `cargo test --test runtime dispatch_close -- --nocapture`

- [ ] **Step 3: Implement Auto, join deadlines, and close state transitions**

Clamp automatic worker count to 64, cancel only pending tickets during close,
and return deterministic timeout or closed-queue errors.

- [ ] **Step 4: Run focused close/auto tests and verify them**

Run: `cargo test --test runtime dispatch_close -- --nocapture`

### Task 5: Publish the Sprint 12 queue/ticket slice

**Files:**
- Create: `examples/dispatch-queue.bn`
- Create: `docs/language/0.3/bndispatch.md`
- Modify: `docs/language/0.3/0.3.md`
- Modify: `docs/language/0.3/keywords.md`
- Modify: `docs/book/en/toc.md`
- Modify: `docs/book/en/08_standard_library_and_host.md`
- Modify: `ongoing/WBS-0.3.md`
- Modify: `ongoing/bucket.md`

**Interfaces:**
- Consumes the completed queue/ticket API.
- Produces a documented local runnable example and acceptance evidence.

- [ ] **Step 1: Write a failing CLI check for the example**

Run: `cargo test --test cli check_dispatch_queue_example_exits_zero -- --nocapture`

- [ ] **Step 2: Add the example and documentation**

The example uses two named no-argument functions, `Queue.Concurrent(2)`, two
tickets, `Dispatch.JoinAll`, and controlled queue close without Internet I/O.

- [ ] **Step 3: Run complete verification**

Run: `cargo fmt --check && cargo test --all-targets --quiet && cargo clippy --all-targets -- -D warnings && git diff --check`
