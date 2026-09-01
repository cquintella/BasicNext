# Basic Next 0.4 Async/Await Design

## Goal

Add a bounded language-level syntax for scheduling named asynchronous
functions on an explicitly selected `BNDispatch` queue and waiting for their
completion. The first 0.4 slice is intended to support opt-in concurrent
`BNWeb` handlers without changing the synchronous 0.3 server contract.

## Contract boundary

`ASYNC` and `AWAIT` are 0.4 language features. They must not be added to the
accepted 0.3 grammar, keyword registry, or 0.3 conformance claim. `BNDispatch`
remains an explicitly imported external module, and `HOST` remains the only
built-in interface object.

The initial feature deliberately does not introduce generic futures, shared
mutable interpreter state, OS thread handles, implicit global queues, or
unbounded waits.

## Surface syntax

An asynchronous function is declared with an `ASYNC` modifier:

```basic
ASYNC FUNCTION ServeClient() AS VOID OR Error
    PRINT "serving"
END FUNCTION
```

The caller supplies the queue explicitly and receives the existing opaque
`Dispatch.Ticket` value:

```basic
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(4)
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue ServeClient()
LET result AS VOID OR Error = AWAIT ticket(60000)
```

The grammar accepts only a named function target. The queue expression and
function arguments are evaluated once, left to right, before submission.

## Semantics and ownership

- An `ASYNC FUNCTION` has a `VOID OR Error` task result in the initial slice.
- Submission returns `Dispatch.Ticket OR Error`; queue bounds and pending-ticket
  limits are enforced by `BNDispatch`.
- The worker executes an isolated module/runtime copy. No BN object, mutable
  variable, interpreter frame, or output writer is shared with the caller.
- Task output is forwarded through the ticket's existing synchronized output
  channel; worker output order is unspecified.
- `AWAIT ticket(timeoutMs)` is bounded by 1–60,000 milliseconds and completes
  with `VOID`, timeout/error, or the retained task error.
- A ticket may be awaited more than once after completion. Waiting does not
  transfer ownership or expose an OS synchronization handle.
- Queue close cancels pending work according to the existing ticket lifecycle;
  running work is not forcefully killed.

## Frontend and IR

The lexer adds `ASYNC` and `AWAIT` as reserved keywords in the 0.4 registry.
The AST records the modifier, queue expression, target function expression,
arguments, timeout expression, and all source spans. The semantic analyzer:

1. restricts `ASYNC FUNCTION` to named, no-shared-state task functions with
   the accepted `VOID OR Error` result;
2. requires an explicitly typed `Dispatch.Queue` operand;
3. validates the target signature and argument types;
4. requires an integer timeout for `AWAIT` and rejects values outside the
   bounded range; and
5. preserves the existing import and external-module identity rules.

IR receives explicit `DispatchSubmit` and `DispatchAwait` operations carrying
the queue/ticket values and source spans. Lowering remains downstream of
semantic analysis; no LLVM lowering is implied by the interpreter-only 0.4
feature until an independent backend contract is accepted.

## Runtime and BNWeb integration

The runtime reuses the native `BNDispatch` provider and its bounded worker
pool. `DispatchSubmit` delegates to the same isolated-module execution path as
`Queue.Async`; `DispatchAwait` delegates to ticket waiting and maps timeout and
task failures to typed `Error` values.

The 0.4 `BNWeb` revision may opt a server into concurrent request handling by
submitting an isolated handler task to an explicit bounded queue. Request
ownership, admission, graceful stop, response completion, transport logging,
and failure ordering must be specified and tested in the BNWeb decision gate.
The 0.3 serial `Server.Dispatch` and transport callback boundary remain
unchanged.

## Verification requirements

The implementation must add positive and negative grammar fixtures, semantic
diagnostics, IR construction tests, runtime tests for submission/await,
timeout, cancellation, task failure, queue closure, output forwarding, and
worker isolation, plus a local `BNWeb` opt-in integration test. Required
repository checks remain:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

The examples must include a client/server program using an explicit concurrent
queue and `AWAIT`, while retaining a runnable synchronous 0.3 example. No
Internet service or platform-specific thread evidence may be required.

## Decision gates

Before implementation, the 0.4 WBS must freeze:

- exact grammar and precedence for `ASYNC` and `AWAIT`;
- whether function parameters are copied, serialized, or restricted to scalar
  and immutable values;
- cancellation and timeout behavior for a running task;
- response ownership and graceful shutdown for concurrent `BNWeb` handlers;
- ordering and failure policy for forwarded output and completed responses; and
- the provider/dependency policy for any new thread or synchronization backend.
