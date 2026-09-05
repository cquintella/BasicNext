# BNDispatch Compiled ABI Design

## Goal

Define a stable C ABI between LLVM-generated Basic Next code and `bn_rt` for
compiled `BNDispatch` queues, tasks, tickets, errors, values, and
synchronization primitives.

## Scope

The ABI covers `Queue.Async`, `Ticket.Wait`, cancellation, queue/ticket
lifecycle, task arguments and results, bounded errors, and Group, Barrier,
Semaphore, and Mutex operations. It does not expose Rust layouts or operating
system handles.

## ABI types

```c
typedef uint64_t BNDispatchHandle;
typedef uint32_t BNDispatchStatus;

enum {
    BN_DISPATCH_OK = 0,
    BN_DISPATCH_ERROR = 1,
    BN_DISPATCH_TIMEOUT = 2,
    BN_DISPATCH_CANCELLED = 3,
    BN_DISPATCH_CLOSED = 4,
    BN_DISPATCH_INVALID_HANDLE = 5,
    BN_DISPATCH_LIMIT = 6
};
```

Values cross the boundary through a tagged representation. Strings and byte
buffers are bounded and copied by the runtime. Handles remain opaque.

```c
typedef enum {
    BN_VALUE_NULL, BN_VALUE_BOOLEAN, BN_VALUE_INTEGER, BN_VALUE_FLOAT,
    BN_VALUE_STRING, BN_VALUE_BYTES, BN_VALUE_HANDLE, BN_VALUE_NA, BN_VALUE_EOF
} BNValueKind;

typedef struct {
    BNValueKind kind;
    uint32_t flags;
    union {
        int64_t integer;
        double floating;
        uint8_t boolean;
        struct { const uint8_t *data; uint32_t length; } bytes;
        BNDispatchHandle handle;
    } payload;
} BNValue;
```

Errors use bounded, runtime-owned messages:

```c
typedef struct {
    uint32_t code;
    const char *message;
    uint32_t message_length;
} BNDispatchError;
```

## Task ABI

LLVM emits one trampoline for each compiled `ASYNC FUNCTION`. The trampoline
converts `BNValue` arguments into Basic Next values, executes the function,
and converts its result or error back into the ABI representation.

```c
typedef BNDispatchStatus (*BNDispatchTaskFn)(
    void *context,
    const BNValue *arguments,
    uint32_t argument_count,
    BNValue *result,
    BNDispatchError *error
);
```

The context is runtime-owned and contains copied arguments, cancellation state,
deadline, output budget, and host/provider references. User code cannot inspect
it directly.

## Queue and ticket ABI

```c
BNDispatchStatus bn_rt_dispatch_queue_create(
    uint32_t workers, BNDispatchHandle *queue);
BNDispatchStatus bn_rt_dispatch_submit(
    BNDispatchHandle queue, BNDispatchTaskFn task, void *context,
    const BNValue *arguments, uint32_t argument_count,
    BNDispatchHandle *ticket);
BNDispatchStatus bn_rt_dispatch_await(
    BNDispatchHandle ticket, int64_t timeout_ms,
    BNValue *result, BNDispatchError *error);
BNDispatchStatus bn_rt_dispatch_cancel(BNDispatchHandle ticket);
BNDispatchStatus bn_rt_dispatch_ticket_close(BNDispatchHandle ticket);
BNDispatchStatus bn_rt_dispatch_queue_join(
    BNDispatchHandle queue, int64_t timeout_ms);
BNDispatchStatus bn_rt_dispatch_queue_close(
    BNDispatchHandle queue, int64_t timeout_ms);
void bn_rt_dispatch_error_free(BNDispatchError *error);
```

`TIMEOUT` leaves a live task intact. Cancellation succeeds only before task
execution begins. Close operations are idempotent and bounded.

## Synchronization ABI

Group, Barrier, Semaphore, and Mutex receive opaque handles and use the same
status, timeout, limit, and close rules. Their exact functions mirror the
interpreter provider methods and are declared in `bn_rt` beside the queue ABI.

## LLVM mapping

`DispatchSubmit` lowers to `bn_rt_dispatch_submit`; `DispatchAwait` lowers to
`bn_rt_dispatch_await`. LLVM owns aggregate conversion and distinct tags for
`Error`, `NA`, and `EOF`. All native examples must match interpreter output,
including timeout, cancellation, task errors, output capture, and lifecycle
behavior.

## Compatibility and limits

The ABI is versioned by the `bn_rt` crate contract, uses only fixed-width C
types, and enforces the configured dispatch limits. No raw Rust type, closure,
thread handle, or unbounded allocation crosses the boundary.
