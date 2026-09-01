# I/O and Concurrency

Basic Next version 0.3 handles input/output (I/O) and concurrency through explicit capabilities and external modules. This design ensures that the core language remains deterministic and predictable, while providing powerful tools for building network services and concurrent programs.

## Synchronous, Bounded I/O

All I/O in Basic Next is synchronous and bounded. The language does not use implicit asynchronous runtimes (like `async/await` in other languages). Instead, operations block until they complete or hit an explicit timeout, returning either the requested data or an explicit `Error` object.

### File System

Access to local files is managed through `HOST.FileSystem`.

```basic
IMPORT HOST.FileSystem AS FS

LET file AS FS.File OR Error = FS.Open("config.txt", FS.READ)
IF file IS Error THEN
    PRINT "Error opening file: " + file.Message
ELSE
    LET data AS STRING OR Error = file.ReadAll()
    file.Close()
END IF
```

### Networking

Raw network access is provided by the native host capability `HOST.Net`. It supports IPv4 and IPv6 addressing, system DNS resolution, TCP, UDP, and bounded ICMP Echo. The operating system owns the underlying network stack.

```basic
IMPORT HOST.Net AS Net
```

For HTTP communication, you should use the `BNWeb` module instead of raw sockets. `BNWeb` consumes `HOST.Net` internally to provide a bounded request/response model, routing, filters, and local HTTP/1.1, HTTP/2, and HTTPS server adapters.

```basic
IMPORT BNWeb AS Web
```

## Concurrency and Parallelism

Basic Next version 0.3 introduces concurrency through the `BNDispatch` module. While the language itself does not have a `PARALLEL` keyword or built-in threads, `BNDispatch` provides a robust, host-backed task dispatcher.

```basic
IMPORT BNDispatch AS Dispatch
```

### BNDispatch

`BNDispatch` provides bounded serial and concurrent queues, named-function tasks, tickets, joins, groups, barriers, semaphores, and mutexes.

These APIs are deliberately separated from the core language and do not expose native thread handles directly to the programmer. Instead, you dispatch tasks to queues.

To determine the available parallel capacity of the host system, you can use `HOST.NumProcs()`, which exposes the logical processor count available to bounded dispatch selection.

```basic
LET cores AS INTEGER OR Error = HOST.NumProcs()
IF cores IS Error THEN
    cores = 2 // Fallback
END IF
```

### Constraints and Resource Management

To prevent resource exhaustion and ensure determinism:
- Queue workers are limited to a maximum of 64.
- Pending work items are limited to 1,024.
- Lifecycle waits use explicit timeouts ranging from 1 to 60,000 milliseconds.
- Synchronization operations (like acquiring a mutex or waiting on a barrier) return an `Error` on timeout or if invalid bounds are supplied.

This explicit error handling forces the application to deal with resource pressure, timeouts, and concurrency limits gracefully, rather than crashing or hanging indefinitely.
