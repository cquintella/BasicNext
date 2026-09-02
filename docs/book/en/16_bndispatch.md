# Appendix J: BNDispatch

`BNDispatch` is a host-backed external module providing concurrency primitives and job orchestration. It is explicitly separated from the core language to ensure safety and determinism.

```basic
IMPORT BNDispatch AS Dispatch
```

## Concurrency without Native Threads

Basic Next does not expose native OS thread handles, nor does it include a `PARALLEL` language keyword in version 0.3. Instead, `BNDispatch` provides an abstraction based on dispatch queues, tasks, and synchronization primitives:

* **Bounded Queues:** You can create serial (one-by-one) or concurrent dispatch queues.
* **Tasks and Tickets:** Tasks are named functions dispatched to a queue. Dispatching a task returns a Ticket, which you can use to `Join` (wait for completion) or `Cancel` the operation.
* **Groups and Barriers:** Used to coordinate multiple tasks, ensuring a set of operations finishes before proceeding.
* **Semaphores and Mutexes:** Standard synchronization primitives to protect shared resources across concurrent tasks.

## Constraints and Resource Limits

To avoid resource exhaustion and deadlocks, `BNDispatch` strictly limits execution contexts and forces bounded wait times:

* **Workers:** The maximum number of queue workers is capped at `64`.
* **Pending Work:** Queues will hold a maximum of `1,024` pending tasks.
* **Timeouts:** Lifecycle waits (such as joining a ticket or acquiring a mutex) require explicit timeouts ranging from `1` to `60,000` milliseconds.

Synchronization operations return an `Error` on timeout or invalid bounds. This forces the programmer to explicitly handle resource pressure:

```basic
// Conceptual example of bounded synchronization
LET mtx AS Dispatch.Mutex = Dispatch.CreateMutex()
LET lock AS VOID OR Error = mtx.Acquire(5000) // 5000ms timeout

IF lock IS Error THEN
    PRINT "Failed to acquire lock: " + lock.Message
ELSE
    // Critical section
    mtx.Release()
END IF
```

## Integration with HOST.NumProcs

For concurrent queues, you can query the host system's logical processor count to tune the number of concurrent workers dynamically, avoiding over-subscription:

```basic
LET cores AS INTEGER OR Error = HOST.NumProcs()
```

## Forward to 0.4

While `BNDispatch` introduces powerful concurrency, full integration of concurrent threading with `BNWeb` transport callbacks remains deferred to version 0.4.
