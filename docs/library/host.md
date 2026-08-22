# HOST capabilities 0.1

## Status

Accepted host contract for Basic Next 0.1. Host capabilities use ordinary
`IMPORT HOST.name AS alias` declarations; there is no separate plugin syntax.

## Command-line environment

`HOST.main` provides the `SYSTEM` object for the executable module:

```basic
IMPORT HOST.main AS main

LET count AS INTEGER = main.ArgumentCount()
LET executable AS STRING = main.Argument(0)
```

`main.ArgumentCount()` includes the executable entry. `main.Argument(0)` is the
executable name or path exactly as supplied by the host, and subsequent indices
are the arguments in their original order. Arguments are immutable strings.
An index smaller than zero or greater than or equal to `ArgumentCount()` raises
`INDEX_OUT_OF_BOUNDS`.

Only the executable module may import `HOST.main`. The reference `bn run`
command always provides it.

## Clocks

`HOST.clock` separates nondeterministic clock acquisition from pure timestamp
conversion:

```basic
IMPORT HOST.clock AS Clock

LET now AS TIMESTAMP = Clock.Timestamp()
LET started AS INT64 = Clock.Monotonic()
```

`Clock.Timestamp()` returns a `TIMESTAMP`: signed milliseconds since
1970-01-01T00:00:00Z. `Clock.Monotonic()` returns a nondecreasing `INT64` count
of nanoseconds from an unspecified origin. A monotonic value measures elapsed
time only; it is not a timestamp and cannot be converted to a calendar date.

The observable clock resolution may be coarser than its return unit. The
reference `bn run` command provides both clocks.

## Availability

An imported host capability is required. Execution fails before `Start` with
`HOST_CAPABILITY_UNAVAILABLE` when the selected host cannot provide it.

`HOST.memory` is not part of Basic Next 0.1. BN-owned typed regions use
`NEW TYPE[count]`. Shared memory, memory-mapped I/O, device buffers, and FFI
memory require a later capability contract.

Files, networking, time zones, concurrency, GPU devices, DOM access, and other
optional capabilities are also outside 0.1.
