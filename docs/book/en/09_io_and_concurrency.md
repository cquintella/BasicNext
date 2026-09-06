# I/O and Concurrency

Basic Next version 0.3 handles input/output (I/O) and concurrency through explicit capabilities and external modules. This design ensures that the core language remains deterministic and predictable, while providing powerful tools for building network services, processing files, and coordinating concurrent tasks.

## Synchronous, Bounded I/O

All I/O in Basic Next is synchronous and bounded. Operations block until they complete or encounter an explicit timeout or failure, returning either the requested data or an explicit `Error` object.

Basic Next separates general file access from tabular data manipulation:
- **`HOST.FileSystem`**: The native host capability for reading and writing raw text and binary files.
- **`BNData`**: The standard external module for columnar tabular data (`DataFrame`) and CSV parsing.

## File System Operations (`HOST.FileSystem`)

File access is managed through the `HOST.FileSystem` capability:

```basic
IMPORT HOST.FileSystem AS FS
```

### Opening and Closing Files

`FS.Open` takes a path and an access mode constant:
- `FS.READ` (`0`): Open an existing file for reading.
- `FS.WRITE` (`1`): Create or truncate a file for writing.
- `FS.APPEND` (`2`): Open or create a file for appending data at the end.

Every `FS.File` instance must be closed with `.Close()` and deterministically deallocated with `DELETE file` to avoid leaking operating system handles:

```basic
LET file AS FS.File OR Error = FS.Open("log.txt", FS.WRITE)
IF file IS Error THEN
    PRINT "Error opening file: " + file.Message
    RETURN
END IF

file.WriteLine("System initialized.")
file.Close()
DELETE file
```

### Text Family vs. Binary Family Rule

When a file handle is opened, it has no assigned family. Upon the first I/O operation, the handle permanently locks into one of two mutually exclusive modes:
- **Text family**: Triggered by calling `ReadLine()`, `ReadAll()`, `Write()`, or `WriteLine()`.
- **Binary family**: Triggered by calling `ReadBytes()` or `WriteBytes()`.

Mixing text and binary methods on the same open handle causes subsequent calls to return an `Error`.

### Reading and Writing Text Files

For reading whole files or line-by-line streaming:

```basic
LET file AS FS.File OR Error = FS.Open("config.txt", FS.READ)
IF file IS Error THEN
    PRINT "Failed to open config."
    RETURN
END IF

// Reading line by line until EOF
REPEAT
    LET line AS STRING OR EOF OR Error = file.ReadLine()
    IF line IS EOF THEN
        EXIT REPEAT
    END IF
    IF line IS Error THEN
        PRINT "Read error: " + line.Message
        EXIT REPEAT
    END IF
    PRINT "Config entry: " + line
END REPEAT

file.Close()
DELETE file
```

To read the entire file content in one call, use `file.ReadAll()`.

### Reading and Writing Binary Files

Binary I/O operates on raw buffers using pointers to byte arrays (`POINTER TO BYTE[]`).

Writing binary bytes:

```basic
LET file AS FS.File OR Error = FS.Open("output.bin", FS.WRITE)
IF file IS Error THEN
    RETURN
END IF

LET buffer AS POINTER TO BYTE[] = NEW BYTE[4]
buffer[0] = 0xDE AS BYTE
buffer[1] = 0xAD AS BYTE
buffer[2] = 0xBE AS BYTE
buffer[3] = 0xEF AS BYTE

LET status AS VOID OR Error = file.WriteBytes(buffer, 4)
IF status IS Error THEN
    PRINT "Write failed: " + status.Message
END IF

file.Close()
DELETE file
DELETE buffer
```

Reading binary bytes into an allocated buffer:

```basic
LET file AS FS.File OR Error = FS.Open("input.bin", FS.READ)
IF file IS Error THEN
    RETURN
END IF

LET buffer AS POINTER TO BYTE[] = NEW BYTE[1024]
LET bytesRead AS INTEGER OR EOF OR Error = file.ReadBytes(buffer)

IF bytesRead IS EOF THEN
    PRINT "File is empty."
ELSE IF bytesRead IS Error THEN
    PRINT "Read error: " + bytesRead.Message
ELSE
    PRINT "Bytes read:", bytesRead
END IF

file.Close()
DELETE file
DELETE buffer
```

### Capability File Helpers

`HOST.FileSystem` also provides standalone utility methods that do not require opening a handle:
- `FS.Exists(path AS STRING) AS BOOLEAN OR Error`: Checks if a file exists on disk.
- `FS.DeleteFile(path AS STRING) AS VOID OR Error`: Removes a file from disk.

```basic
IF FS.Exists("temp.dat") = TRUE THEN
    FS.DeleteFile("temp.dat")
END IF
```

## Networking

Raw network access is provided by the native host capability `HOST.Net`. It supports IPv4 and IPv6 addressing, system DNS resolution, TCP, UDP, and bounded ICMP Echo. The operating system owns the underlying network stack.

```basic
IMPORT HOST.Net AS Net
```

For HTTP communication, use the `BNWeb` module instead of raw sockets. `BNWeb` consumes `HOST.Net` internally to provide a bounded request/response model, routing, filters, and local HTTP/1.1, HTTP/2, and HTTPS server adapters.

```basic
IMPORT BNWeb AS Web
```

## Concurrency and Parallelism

Basic Next version 0.3 introduces concurrency through the `BNDispatch` module. While the language itself does not have a `PARALLEL` keyword or built-in threads, `BNDispatch` provides a robust, host-backed task dispatcher.

```basic
IMPORT BNDispatch AS Dispatch
```

### BNDispatch Queues and Tasks

`BNDispatch` provides bounded serial and concurrent queues, named-function tasks, tickets, joins, groups, barriers, semaphores, and mutexes.

These APIs are deliberately separated from the core language and do not expose native thread handles directly to the programmer. Instead, tasks are dispatched to queues.

To determine the available parallel capacity of the host system, use `HOST.NumProcs()`, which exposes the logical processor count available to bounded dispatch selection:

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
- Synchronization operations (such as acquiring a mutex or waiting on a barrier) return an `Error` on timeout or if invalid bounds are supplied.

This explicit error handling forces applications to handle resource pressure and concurrency limits cleanly, rather than crashing or hanging indefinitely.
