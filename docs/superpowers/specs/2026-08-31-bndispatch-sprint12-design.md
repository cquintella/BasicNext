# BNDispatch Sprint 12 Design

## Goal

Add the external `BNDispatch` module and a native-interpreter provider for
bounded Grand-Central-Dispatch-style queues. The first release schedules named
BN functions with no arguments and `VOID OR Error` results.

## Boundaries

`BNDispatch` is an explicit import and never a language-core namespace.
`HOST.NumProcs()` remains the only new built-in form in this sprint. A queue
owns no operating-system thread handle visible to BN. A task runs in a fresh,
isolated interpreter executor; task state, objects, memory, and module statics
are never shared with the submitting executor.

The worker count is 1 through 64 and each queue accepts at most 1,024 pending
tickets. `Queue.Auto()` uses `HOST.NumProcs()` and clamps the result to 64.
All waits accept 1 through 60,000 milliseconds. No wait is infinite.

## Public API

```basic
IMPORT BNDispatch AS Dispatch

Queue.Serial() AS Queue OR Error
Queue.Concurrent(workers AS INTEGER) AS Queue OR Error
Queue.Auto() AS Queue OR Error

queue.Async(work AS FUNCTION() AS VOID OR Error) AS Ticket OR Error
queue.Join(timeoutMs AS INTEGER) AS VOID OR Error
queue.Close(timeoutMs AS INTEGER) AS VOID OR Error

ticket.Id() AS INTEGER
ticket.Status() AS INTEGER
ticket.Wait(timeoutMs AS INTEGER) AS VOID OR Error
ticket.Cancel() AS BOOLEAN OR Error
ticket.Error() AS Error OR NA
ticket.IsDone() AS BOOLEAN
ticket.Close() AS VOID
```

`PENDING`, `RUNNING`, `COMPLETED`, `FAILED`, and `CANCELLED` are exported
integer constants. A failed task preserves its `Error`; cancellation succeeds
only while a ticket is pending. Queue closure cancels pending work and waits
for running work only through its deadline.

## Execution and output

The provider stores only the fully qualified function name. A worker creates a
fresh executor for the shared immutable IR module, invokes that function with
no arguments, and records completion. Worker output is forwarded through a
synchronized queue to the submitting interpreter output stream; ordering
between workers is intentionally unspecified. A task cannot access caller
locals, objects, heap allocations, open files, sockets, or module statics.

This preserves memory safety and makes failure/cancellation observable through
the ticket. Shared-state primitives, groups, queue barriers, participant
barriers, semaphores, mutexes, and BNWeb integration remain subsequent Sprint
12 tasks after the queue/ticket slice is proven.

## Errors and lifecycle

Invalid worker count, queue saturation, invalid timeout, task signature,
closed queue, closed ticket, task failure, and task timeout produce typed
`Error` values. `Close()` makes all later ticket operations fail. Completed
tickets retain only status and an optional error until explicitly closed.

## Verification

Tests use only local worker execution. They prove parallel execution capacity,
serial FIFO execution, ticket state transitions, task failure propagation,
pending-only cancellation, queue limit, join timeout, close behavior, and
`Queue.Auto()` clamping. No public service or operating-system thread ID is
part of an assertion.
