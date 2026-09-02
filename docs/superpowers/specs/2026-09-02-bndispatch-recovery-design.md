# BNDispatch 0.4 Recovery Design

## Status and authority

Status: **ACCEPTED FOR PLANNING — 2026-09-02.**

This decision reopens the BNDispatch portion of 0.4 after a source audit
found that the provider does not meet its accepted lifecycle and bounded-resource
contract. It supersedes the BNDispatch-specific implementation claims in
`ongoing/0.4-concurrency-decision.md`; the grammar, `ASYNC`/`AWAIT` syntax, and
the isolated-executor BNWeb callback boundary remain accepted.

The work is a release gate. `ASYNC`/`AWAIT` remains specified, but 0.4 cannot
claim final conformance for its native BNDispatch provider until this design is
implemented and its acceptance evidence passes.

## Confirmed audit findings

| Identifier | Finding | Required outcome |
|---|---|---|
| BN-DISPATCH-001 | `Queue.Close` neither disconnects its sender nor joins its worker threads | Close cancels pending work, disconnects workers, drains/join workers before success, and reports deadline expiry honestly |
| BN-DISPATCH-002 | A panicking job kills a worker and can strand its ticket in `RUNNING` | Every job is panic-isolated; its ticket becomes `FAILED`; later work remains serviceable |
| BN-DISPATCH-003 | Queue capacity is computed from retained ticket history | Only non-terminal tickets consume pending capacity |
| BN-DISPATCH-004 | One barrier timeout can make peer waiters observe false success | A timed-out generation is broken; every remaining participant receives `Error` |
| BN-DISPATCH-005 | Semaphore permits can exceed their initial capacity | Release above the initial capacity returns `Error` and preserves the cap |
| BN-DISPATCH-006 | Mutex unlock and group leave have no invalid-owner/underflow error channel | Unlock by a non-owner and Leave without an outstanding Enter return `Error` |
| BN-DISPATCH-007 | Dispatch resource thresholds are partly hardcoded and output reaches through BNWeb configuration | A typed `DispatchLimits` snapshot owns every dispatch limit |
| BN-DISPATCH-008 | Ticket close destroys retained failure information | Closing a ticket releases task/output payload only; a bounded failure diagnostic remains observable |
| BN-DISPATCH-009 | Async executor isolation is structural but lacks direct regression evidence | Tests prove independent executor heap, output, resources, and host-random state per task |

The audit claim that pending cancellation still executes the user task is not
confirmed: the existing job wrapper calls `mark_running` before user work and
returns when the ticket is already cancelled. The replacement state machine
must nevertheless make that property explicit and race-safe.

## Approved public-contract amendments

The 0.4 BNDispatch module signatures become:

```basic
PUBLIC FUNCTION Leave() AS VOID OR Error
PUBLIC FUNCTION Release() AS VOID OR Error
PUBLIC FUNCTION Unlock() AS VOID OR Error
```

Ignoring these return values remains valid BN source, but callers can now
observe misuse. No keyword, new built-in, dependency, or `unsafe` code is
introduced.

`Ticket.Close()` keeps its `VOID` signature. After close, `Task()` and retained
output are unavailable, while `Status()` and `Error()` continue to expose the
terminal result or bounded failure diagnostic.

## Queue architecture

`QueueInner` owns one state machine guarded by a mutex and one condition
variable. It contains:

```text
closed: bool
sender: Option<SyncSender<Job>>
workers: Vec<JoinHandle<()>>
pending: HashMap<TicketId, Arc<TicketInner>>
worker_identity: per-worker logical identity
```

`pending` is the sole capacity registry. A ticket leaves it exactly once when
it becomes terminal. Callers retain their own opaque ticket handle through the
executor handle table; queue capacity does not retain completed history.

`Queue.Close(timeoutMs)` has these ordered operations:

1. reject new submissions;
2. transition every `PENDING` ticket to `CANCELLED`;
3. remove the sender so workers observe channel closure after queued wrappers;
4. wait for running tickets and workers until the common deadline;
5. join every finished worker; return `Error` on deadline expiry without
   reporting a successful close.

Close is idempotent. A close initiated by one of that queue's workers returns
`Error` rather than joining itself. Running work is cooperative and is never
force-killed.

## Ticket state machine and panic policy

Only these transitions are legal:

```text
PENDING -> RUNNING | CANCELLED
RUNNING -> COMPLETED | FAILED
```

All transition methods return a result that distinguishes a stale transition
from a closed resource. The worker must claim `PENDING -> RUNNING` before it
enters user code. A cancelled wrapper exits without invoking the task body.
The worker invokes the task through `catch_unwind(AssertUnwindSafe(...))`; a
panic maps to a bounded `FAILED` diagnostic. A panic never poisons queue state
or permanently lowers the advertised worker capacity.

Task output is a ticket-owned bounded byte buffer. `Ticket.Wait` consumes the
output once after completion; `Queue.Join` only waits and never duplicates
previously observed task output.

## Synchronization semantics

Each executor and queue worker receives a stable logical execution identity.
`DispatchMutex` records that identity on Lock and rejects Unlock from any
other identity. It is non-reentrant unless a future contract explicitly adds
reentrancy.

`DispatchGroup` counts outstanding Enter operations and rejects Leave when the
count is zero. Group waiting remains open to any holder of the group handle.

`DispatchSemaphore` stores both initial and available permits. Release above
the initial maximum is an error and does not modify available permits.

`Barrier` records a generation outcome. If a waiter times out before all
parties arrive, that generation becomes broken, all waiters are notified, and
every waiter in the generation receives the same timeout/error outcome. The
next caller begins a fresh generation only after prior waiters have observed
the broken outcome.

## Configuration and isolation

`config/0.4-bnweb-limits.toml` receives a complete `[dispatch]` section for
worker maxima, pending-ticket maxima, timeout range, and output bytes.
`src/config.rs` exposes immutable `DispatchLimits`; Queue, Barrier, Group,
Semaphore, Mutex, and async output consume that snapshot. `dispatch.rs` holds
no normative resource literal and never calls `web_limits()`.

Every async task receives a cloned module, a fresh `Executor`, private input,
private bounded output, and a task-local HostEnv. A task-local HostEnv must
derive an independent random seed rather than copy the caller's current PRNG
state. No caller heap, handle table, writer, or mutable HostEnv is captured.

## Required acceptance tests

- Close a queue with idle workers and prove every worker is joined before
  Close returns success; repeat Close and prove idempotence.
- Block one running job, close with a short deadline, and prove Close returns
  timeout rather than success; release it and prove a later Close joins it.
- Cancel a queued job before it claims `RUNNING`; assert a shared test marker
  remains untouched and status is `CANCELLED`.
- Panic one job; assert its ticket is `FAILED`, a later job completes, and the
  worker pool remains at configured capacity.
- Complete more than the pending limit sequentially; assert every submission
  is accepted once prior tickets are terminal. Saturate truly simultaneous
  pending jobs and assert deterministic rejection.
- Run two real waiters against a barrier where one expires; assert neither
  waiter returns `Ok(false)` and the next generation can complete normally.
- Assert foreign Unlock, excess Release, and unmatched Leave return typed
  errors without mutating ownership, permits, or count.
- Assert Ticket.Close preserves a terminal error but removes bounded output.
- Run two async tasks that allocate/mutate equivalent local values and emit
  different output; assert no heap/output/resource cross-talk and independent
  host-random streams.
- Run the full 0.4 Rust, plugin, formatting, Clippy, and diff gates after the
  focused suite passes.

## Release decision

The 0.4 release notes and README must identify BNDispatch conformance as
reopened until this work closes. The source release is not withdrawn; its
BNWeb, DAP, Jupyter, and non-dispatch evidence remains recorded separately.
