# BNDispatch (external module)

`BNDispatch` is an external provider. Programs must explicitly import it; it
is not part of the language core. Implementations expose bounded queue and
synchronization handles, while the host owns native resources.

## Queue and ticket

`Queue.Serial()`, `Queue.Concurrent(workers)`, and `Queue.Auto()` create queues.
Workers are limited to 1..64 and pending tickets to 1024. `Async(function)`
returns an opaque `Ticket`. `Ticket.Wait`, `Cancel`, `Status`, `Error`,
`IsDone`, and `Close` provide bounded lifecycle control. `Join(timeoutMs)` and
`Close(timeoutMs)` wait for terminal work; timeout values are 1..60000 ms.

## Synchronization

`Group.New()` supports `Enter`, `Leave`, and `Wait`. `Barrier.New(parties)`
coordinates a bounded number of participants. `Semaphore.New(permits)`
provides bounded `Acquire(timeoutMs)` and `Release`; `Mutex.New()` provides
bounded `Lock(timeoutMs)` and `Unlock`.

The 0.4 provider recovery amendment makes `Leave`, `Release`, and `Unlock`
return `VOID OR Error`: unmatched group leave, excess semaphore release, and
unlock by a non-owner are rejected. The 0.3 source contract remains otherwise
unchanged; hosts implementing only 0.3 may retain the original signatures.

All handles are opaque and must not be inspected as operating-system thread
handles. Invalid bounds and expired waits return `Error`.
