# BNDispatch Standard Library (0.6.2)

`BNDispatch` is an explicitly imported external module. `ASYNC` and `AWAIT`
submit work to its queues and wait for their tickets ([0.6.md](0.6.md),
"Bounded `ASYNC`/`AWAIT`"). This file specifies the classes and their
failures; both backends implement it.

```basic
IMPORT BNDispatch AS Dispatch
```

## Bounds

| Bound | Value |
| --- | --- |
| Workers of a `Concurrent` queue | 1 through 64 |
| Pending tickets per queue | 1024 |
| Barrier parties | 1 through 64 |
| Semaphore permits | 1 through 1024 |
| Every `timeoutMs` argument | 1 through 60,000 milliseconds |

A value outside its bound is an `Error` (`INVALID_ARGUMENT`) at run time. An
`AWAIT` timeout written as a literal outside 1 through 60,000 is a static
error (`AWAIT_TIMEOUT`).

## Queue and Ticket

| Member | Result | Rule |
| --- | --- | --- |
| `Queue.Serial()` | `Queue OR Error` | one worker |
| `Queue.Concurrent(workers)` | `Queue OR Error` | `workers` within bounds |
| `Queue.Auto()` | `Queue OR Error` | one worker per available processor, at most 64 |
| `queue.Async(Fn, args…)`, `ASYNC queue Fn(args…)` | `Ticket OR Error` | `CLOSED` on a closed queue; `SATURATED` when 1024 tickets are pending |
| `queue.Join(timeoutMs)` | `VOID OR Error` | waits until every ticket is done |
| `queue.Close(timeoutMs)` | `VOID OR Error` | cancels pending tickets, then waits for running ones; running work is not killed |
| `ticket.Wait(timeoutMs)`, `AWAIT ticket(timeoutMs)` | `VOID OR Error` / `T OR Error` | the worker's result, or the failure below |
| `ticket.Cancel()` | `BOOLEAN OR Error` | `TRUE` when the ticket was pending and is now cancelled; `FALSE` when it already ran |
| `ticket.Error()` | `Error OR NA` | the `Error` the worker returned, unchanged; `NA` otherwise |
| `ticket.Status()` | `INTEGER` | 0 pending, 1 running, 2 completed, 3 failed, 4 cancelled |
| `ticket.IsDone()` | `BOOLEAN` | completed, failed, or cancelled |
| `ticket.Close()` | `VOID` | releases the ticket; `Wait`, `Cancel`, and `AWAIT` on it are then `CLOSED` |

A worker may not `Join` or `Close` its own queue (`INVALID_STATE`): it would
wait for itself.

## Synchronization

| Member | Result | Rule |
| --- | --- | --- |
| `Group.New()` | `Group OR Error` | a counter starting at 0 |
| `group.Enter()` | `VOID` | adds 1 |
| `group.Leave()` | `VOID OR Error` | subtracts 1; `INVALID_STATE` when the count is 0 |
| `group.Wait(timeoutMs)` | `VOID OR Error` | waits until the count is 0 |
| `Barrier.New(parties)` | `Barrier OR Error` | `parties` within bounds |
| `barrier.Wait(timeoutMs)` | `BOOLEAN OR Error` | returns when `parties` callers have arrived: `TRUE` for the last to arrive, `FALSE` for the others. A timeout breaks the round: every caller waiting in it gets `TIMEOUT`, and the next call starts a new round |
| `Semaphore.New(permits)` | `Semaphore OR Error` | `permits` within bounds, all available |
| `semaphore.Acquire(timeoutMs)` | `VOID OR Error` | takes one permit, waiting while none is available |
| `semaphore.Release()` | `VOID OR Error` | returns one permit; `INVALID_STATE` when all `permits` are already available |
| `Mutex.New()` | `Mutex OR Error` | unlocked |
| `mutex.Lock(timeoutMs)` | `VOID OR Error` | waits while it is locked, including by the caller (not re-entrant) |
| `mutex.Unlock()` | `VOID OR Error` | `INVALID_STATE` unless the calling task holds the lock |

## Errors

`Code` of a BNDispatch `Error` ([error.md](error.md)) is one of these
`INTEGER` constants of the module (`Dispatch.TIMEOUT` under
`IMPORT BNDispatch AS Dispatch`). `Operation` names the member
(`BNDispatch.Semaphore.Release`).

| Constant | Value | When |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | A worker count, barrier parties, semaphore permits, or timeout outside its bound |
| `TIMEOUT` | 2 | `AWAIT`, `Wait`, `Join`, `Close`, `Acquire`, or `Lock` ran past its timeout |
| `CLOSED` | 3 | Submitting to a closed queue; waiting on, cancelling, or awaiting a closed ticket |
| `SATURATED` | 4 | The queue already holds 1024 pending tickets |
| `CANCELLED` | 5 | Waiting on a ticket that was cancelled |
| `TASK_FAILED` | 6 | The worker returned an `Error`; `Cause` carries its code and message, and `ticket.Error()` returns it unchanged |
| `INVALID_STATE` | 7 | `Group.Leave` at 0, `Semaphore.Release` with every permit available, `Mutex.Unlock` by a task that does not hold it, a queue joined or closed from its own worker |
| `UNAVAILABLE` | 8 | The system cannot provide the operation: no processor count for `Queue.Auto()`, no entropy for ticket identifiers |
