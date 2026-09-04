# HOST capabilities

## Status

Accepted host contract for Basic Next 0.1, with the 0.2 capabilities in
[Random (0.2)](#random-02), [File system (0.2)](#file-system-02), and the
amended [Console](#console) section. Host capabilities use ordinary
`IMPORT HOST.name AS alias` declarations; there is no separate plugin syntax.

## Command-line environment

0.2 withdraws `HOST.Main` (`SYSTEM`, `ArgumentCount`, `Argument`). The
executable module reads arguments through `HOST.Args`. No `IMPORT` is
required. Other modules must not name `HOST.Args`.

```basic
FUNCTION Start() AS VOID
    PRINT HOST.Args[0]
    PRINT LEN(HOST.Args)
END FUNCTION
```

`HOST.Args[0]` is the absolute path of the executed file (`.bn` source under
`bn run`; the produced binary after `bn build`). The path is rooted: `/...`
on Unix, `C:\...` (or another drive root) on Windows. `HOST.Args[1]` and
later are the program arguments (`bn run file.bn -- ...`). `LEN(HOST.Args)`
includes entry `0`. An out-of-range index raises `INDEX_OUT_OF_BOUNDS`.

`HOST.Args` is not a value: only `LEN(HOST.Args)` and `HOST.Args[index]`
are valid. Indexing is read-only.

The reference `bn run` command always provides `HOST.Args` to the executable
module.

## Console

`HOST.Console` is the named console capability. Capability names after `HOST.`
use an initial capital letter (`HOST.Args`, `HOST.Clock`, `HOST.Console`,
`HOST.Random`, `HOST.FileSystem`; later `HOST.Network`).

0.1 statements `CLS(HOST.Console)` and `BEEP(HOST.Console)` are withdrawn in
0.2. Console operations are methods. `PRINT` and `INPUT()` remain the default
stream macros and do not require the import.

```basic
IMPORT HOST.Console AS CON

CON.Cls()
CON.Beep()
CON.PrintAt(1, 1, "ready")
LET cols AS INTEGER = CON.NumCols()
LET rows AS INTEGER = CON.NumRows()
```

`HOST.Console.Cls()` and `HOST.Console.Beep()` are also valid without an
alias. `Cls` writes the portable ANSI erase-display-and-home sequence;
`Beep` writes ASCII BEL. Both remain available when standard output is
piped.

`PrintAt`, `NumCols`, and `NumRows` require a TTY. If a call executes and
stdout is not a TTY, that call raises `HOST_CAPABILITY_UNAVAILABLE`. A
call that does not run does not fail. The method contract is
[console.md](console.md).

The reference `bn run` command provides `HOST.Console`.

## Clocks

`HOST.Clock` separates nondeterministic clock acquisition from pure timestamp
conversion:

```basic
IMPORT HOST.Clock AS Clock

LET now AS TIMESTAMP = Clock.Now()
LET started AS INT64 = Clock.Timer()
```

`Clock.Now()` returns a `TIMESTAMP`: signed milliseconds since
1970-01-01T00:00:00Z. `Clock.Timer()` returns a nondecreasing `INT64` count
of nanoseconds from an unspecified origin. A monotonic value measures elapsed
time only; it is not a timestamp and cannot be converted to a calendar date.

The observable clock resolution may be coarser than its return unit. The
reference `bn run` command provides both clocks.

## Random (0.2)

```basic
IMPORT HOST.Random AS R

LET unit AS FLOAT = R.Random()
R.Seed(1)
```

Any module may import `HOST.Random` (same rule as `HOST.Clock`, not
`HOST.Main`).

| Method | Meaning |
| --- | --- |
| `Random() AS FLOAT` | One value in `[0, 1)`. |
| `Seed(n AS INTEGER) AS VOID` | Makes the subsequent sequence deterministic. |

Without `Seed`, the host chooses a non-zero initial state once per process.
Tests inject a provider, the same idea as `HOST.Clock`. `RND` is not a
`BNMath` name.

After `Seed(n)`, the sequence is deterministic and is the same in the
reference interpreter and in `bn build`. The generator is **xorshift64\***
with a 64-bit state:

1. Let `state` be `n` converted to `INT64` (0.1 conversion) and then
   reinterpreted as unsigned 64-bit. If `state` is `0`, set `state` to `1`.
2. Each `Random()` updates `state`:
   `state = state XOR (state shifted right 12 bits)`;
   `state = state XOR (state shifted left 25 bits)`;
   `state = state XOR (state shifted right 27 bits)`;
   then multiply `state` by the constant `0x2545F4914F6CDD1D` (unsigned
   64-bit wrap).
3. The return value is the top 53 bits of that product, divided by `2^53`,
   as `FLOAT` in `[0, 1)`.

Shifts are logical. The reference `bn run` command provides `HOST.Random`.

## File system (0.2)

File access is a host capability. It is not a set of keywords. `OPEN`,
`CLOSE`, `READ`, and `WRITE` are not reserved words.

```basic
IMPORT HOST.FileSystem AS FS

LET file AS FS.File OR Error = FS.Open("data.txt", FS.READ)
IF file IS Error THEN
    PRINT file.Message
    RETURN
END IF
LET closed AS VOID OR Error = file.Close()
IF closed IS Error THEN
    PRINT closed.Message
END IF
DELETE file
```

Mode constants are `INTEGER` members of the capability object:

| Name | Meaning |
| --- | --- |
| `FS.READ` | `0` — open for reading |
| `FS.WRITE` | `1` — create or truncate for writing |
| `FS.APPEND` | `2` — write at end; create if missing |

`FS.Open(path AS STRING, mode AS INTEGER) AS FS.File OR Error` returns a
file handle or an `Error`. An unknown mode is a static error when the mode
is a literal that is not one of the three constants; a computed unknown mode
returns `Error`.

`FS.File` is a class. `NEW FS.File()` is legal: it constructs a **closed**
file (no operating-system handle, no path). Methods other than `Close` on a
closed file return `Error`. `FS.Open` constructs and returns a different,
open instance.

Every `FS.File` instance must be released with `DELETE`, including instances
returned by `Open` and instances created with `NEW`. `DELETE` runs the
destructor described below. Process exit does not close leftover handles.
A second `DELETE` raises `DOUBLE_DELETE`, as for any class.

A user class must not `EXTENDS FS.File`. Hold a `File` in a field instead.

`FS.Open` is a factory on the capability. It does not store the result into
an existing closed instance; it allocates a new object. Replacing a
variable that already holds a `File` without `DELETE` leaks that object.

`FS.File` methods:

| Method | Meaning |
| --- | --- |
| `Close() AS VOID OR Error` | Flushes (`sync_all` / `fsync`) and releases the handle. A flush failure returns `Error`; the handle is still released. Idempotent: a closed file returns success (`VOID`), not `Error`. |
| `ReadLine() AS STRING OR EOF OR Error` | One text line without the line ending; `EOF` at end of file. A closed file, a binary-family file, or an I/O failure is `Error`. |
| `ReadAll() AS STRING OR Error` | Remaining text contents. |
| `ReadBytes(buffer AS POINTER TO BYTE[]) AS INTEGER OR EOF OR Error` | Fills `buffer`; returns the byte count, or `EOF` when no bytes remain. A closed file, a text-family file, or an I/O failure is `Error`. |
| `Write(text AS STRING) AS VOID OR Error` | Writes text, no extra newline. |
| `WriteLine(text AS STRING) AS VOID OR Error` | Writes text plus one line ending. |
| `WriteBytes(buffer AS POINTER TO BYTE[], count AS INTEGER) AS VOID OR Error` | Writes `count` bytes from `buffer`. `LEN(buffer)` is the 0.2 region length. `count` must be in `0` through `LEN(buffer)` inclusive; otherwise `INDEX_OUT_OF_BOUNDS`. |

After the first successful text method (`ReadLine`, `ReadAll`, `Write`,
`WriteLine`) a file is in **text** use. After the first successful byte
method (`ReadBytes`, `WriteBytes`) it is in **binary** use. A later call
from the other family returns `Error`. `EOF` from `ReadLine` or
`ReadBytes` is a successful family method. Unused and closed files have no
family.

`FUNCTION DESTRUCTOR` of `FS.File` closes the operating-system handle if it
is still open. If that close fails, `DELETE` still finishes (the object is
released); the failure is not an `Error` value on `DELETE`. Call `Close`
explicitly when the program must observe flush failure.

Paths are `STRING`. Text is UTF-8. Invalid UTF-8 on a text read returns
`Error`.

Capability-level helpers, without a file object:

| Method | Meaning |
| --- | --- |
| `Exists(path AS STRING) AS BOOLEAN OR Error` | `TRUE` if a regular file exists at `path`. Missing path or a directory at `path` is `FALSE`. Permission or other I/O failure is `Error`. |
| `DeleteFile(path AS STRING) AS VOID OR Error` | Removes a file. A missing path is `Error`. |

Outside 0.2: `ChangeDirectory`, directory create/list/delete, `Move`,
`Copy`, `chmod` / `chown`, `Seek`, `Flush`, and a `Path` object.

The Jupyter 0.2 kernel and a `wasm32` host that does not grant files fail
before `Start` with `HOST_CAPABILITY_UNAVAILABLE` when a module imports
`HOST.FileSystem`. The reference `bn run` command provides it for the local
process, subject to ordinary OS permissions.

## Availability

An imported host capability is required. Execution fails before `Start` with
`HOST_CAPABILITY_UNAVAILABLE` when the selected host cannot provide it.

`HOST.Memory` is not part of Basic Next 0.1 or 0.2. BN-owned typed regions
use `NEW TYPE[count]`. Shared memory, memory-mapped I/O, device buffers, and
FFI memory require a later capability contract.

Networking, time zones, concurrency, GPU devices, DOM access, and other
optional capabilities remain outside 0.2.
