# The Standard Library and Host

Basic Next decouples the core language from the operating system. `HOST` is
the only built-in interface object. All `BN*` facilities are external modules
and must be imported explicitly; their detailed contracts live in separate
appendices.

## The `BNMath` namespace

The mathematics standard library is the `BNMath` module. Every use requires
an import; the alias is used for member calls.

```basic
IMPORT BNMath AS Math
LET root AS FLOAT = Math.SQRT(9.0)
LET rounded AS FLOAT = Math.ROUND(3.14159, 2)
LET lower AS FLOAT = Math.FLOOR(3.9)
```

`BNMath` provides strict IEEE 754 implementations for common mathematical functions. Operations return `FLOAT` unless restricted to integers.

The available surface includes:
- Mathematical and exponential functions: `ABS`, `MIN`, `MAX`, `SIGN`, `FLOOR`, `CEIL`, `TRUNC`, `ROUND`, `SQRT`, `HYPOT`, `FMA`, `EXP`, `LOG`, `LOG10`, `LOG2`, `POW`.
- Trigonometry: `SIN`, `COS`, `TAN`, `ASIN`, `ACOS`, `ATAN`, `ATAN2`.
- Text parsing: `VAL(text AS STRING) AS FLOAT`.
- Descriptive statistics: `MEAN`, `MEDIAN`, `MODE`, `STDEV`, `VARIANCE`, `RANGE`, `QUARTILE1`, `QUARTILE3`.
- Range constants: `MAX_INTEGER`, `MIN_INTEGER`, `MAX_FLOAT`, `MIN_FLOAT` (and width-specific variants).

## The `BNString` module

`BNString` is an external module that wraps the primary `STRING` value in the
ARC-managed `S.String` object. Import it explicitly when you need fluent string
operations:

```basic
IMPORT BNString AS S

LET text AS S.String = NEW S.String("  Olá,BN  ")
LET clean AS S.String = text.Trim()
PRINT clean.LowCaps().ToString()
PRINT clean.Contains("BN")

LET words AS S.Tokenizer OR NULL = clean.Tokenizer(",")
IF words IS NOT NULL THEN
    PRINT words.Next()
END IF
```

`Len`/`Length` report Unicode scalar values, `LowCaps` and `HiCaps` perform
Unicode-aware case conversion, and `Tokenizer` walks separator-delimited tokens.
`S.String` instances follow the same ARC rules as other class objects: strong
bindings retain them, scope exit releases them, and weak bindings become `NULL`
when the last strong reference is gone. See the full API in
[`docs/library/bnstring.md`](../../library/bnstring.md) and the runnable
[`bnstring_tour.bn`](../../../examples/bnstring_tour.bn).

## HOST capabilities

Interaction with the underlying operating system—such as reading command-line arguments or checking the system clock—requires explicitly importing a capability from the `HOST` root.

### `HOST.Args`

`HOST.Args` provides access to the arguments passed by the host. **Only the executable module** (the module containing the `Start` function) is permitted to access `HOST.Args`. It does not require an `IMPORT` statement.

You access the arguments using indexing (`HOST.Args[index]`) and check the total number using `LEN(HOST.Args)`. Index `0` is the absolute executable name or path supplied by the host. Further program arguments follow `--`.

```basic
FUNCTION Start() AS VOID
    IF LEN(HOST.Args) > 1 THEN
        PRINT "First user argument: " + HOST.Args[1]
    END IF
END FUNCTION
```

### `HOST.Clock`

```basic
IMPORT HOST.Clock AS Clock
```

`HOST.Clock` provides time measurements:
- `Clock.Now() AS TIMESTAMP`: Returns the current signed UTC Unix-epoch time in milliseconds.
- `Clock.Timer() AS INT64`: Returns a monotonically increasing count of nanoseconds. It does not represent a calendar time and is only used for measuring elapsed durations safely.

### `HOST.NumProcs`

`HOST.NumProcs()` needs no import and returns the logical processor count
available to the current process. This respects host or container limits where
they are reported, so it is appropriate for selecting a bounded worker-pool
size rather than detecting physical CPU cores. It is available in the native
interpreter.

```basic
LET workers AS INTEGER OR Error = HOST.NumProcs()
```

### `HOST.Console`

The runtime provides a default console used implicitly by `PRINT` and `INPUT()`. To interact explicitly with the terminal window, use the `HOST.Console` capability. 

`HOST.Console` provides methods for clearing the screen, emitting a beep, positioning the cursor, and querying the window size:

```basic
IMPORT HOST.Console AS CON

CON.Cls()
CON.Beep()
CON.PrintAt(1, 1, "Hello at top-left") // 1-based coordinates
LET cols AS INTEGER = CON.NumCols()
LET rows AS INTEGER = CON.NumRows()
```

### `HOST.Random`

`HOST.Random` provides pseudorandom number generation.

```basic
IMPORT HOST.Random AS R

LET chance AS FLOAT = R.Random() // Returns a FLOAT in [0, 1)
R.Seed(42) // Explicitly seed the generator
```

### `HOST.FileSystem`

`HOST.FileSystem` provides access to the local file system. The `FS.File` class is used to read and write files.

```basic
IMPORT HOST.FileSystem AS FS

LET file AS FS.File OR Error = FS.Open("data.txt", FS.READ)
IF file IS Error THEN
    PRINT "Could not open file"
ELSE
    LET content AS STRING OR Error = file.ReadAll()
    file.Close()
END IF
```


### `HOST.Exec` (0.5.1)

> **Status:** Shipped in 0.5.1. The capability is available on both the
> interpreter and the native (LLVM) path, with the E01–E14 acceptance matrix
> green on both. Execution is governed by the execution policy (restricted
> profiles deny it by default). Contract: [0.5 language specification — `HOST.Exec`](../../language/0.5/0.5.md#hostexec-051) and [`host-exec.md`](../../library/host-exec.md).

`HOST.Exec` runs an **external program** and captures its output, in the style of a language-level `exec()`: the host **spawns** a child, **waits** until it finishes, and returns a structured result. It does **not** replace the Basic Next process image (that would be POSIX `execve`, which is out of 0.5.1).

```basic
IMPORT HOST.Exec AS Exec

FUNCTION Start() AS VOID
    LET args AS STRING[1]
    args[0] = "-s"
    LET r AS Exec.Result OR Error = Exec.Run("uname", args)
    IF r IS Error THEN
        PRINT r.Message
        STOP 1
    END IF
    PRINT r.ReturnCode
    PRINT r.Stdout
END FUNCTION
```

| Piece | Contract |
| --- | --- |
| Import | `IMPORT HOST.Exec AS Exec` — no new reserved word |
| Primary API | `Run(program AS STRING, args AS STRING[]) AS Exec.Result OR Error` |
| `Exec.Result` | `ReturnCode AS INT64` (child return code), `Stdout AS STRING`, `Stderr AS STRING` |
| Child stdin | Closed (no input blob in 0.5.1) |
| Shell | Forbidden as the implementation of `Run` (no `sh -c` / `cmd /c` default) |
| Non-zero child code | Still a `Result` — inspect `ReturnCode`; `Error` is for Host/OS launch or capture failure |
| Signal termination (POSIX) | `ReturnCode = -signal` (e.g. `-15` for SIGTERM); the completed spawn stays a `Result` |
| Capture limits | 16 MiB per stream and a 60 s wall-clock ceiling by default; policy may lower them. Overflow/timeout returns a stable `Error`, never truncated output |
| Restricted profiles | Deny `HOST.Exec` by default (for example Jupyter-style hosts) |

Do not confuse `HOST.Exec` with `HOST.SQLite` / `Db.Exec(sql)` (SQL execution), which is a different capability.

### `HOST.Net`

`HOST.Net` is a native-host capability added in version 0.3 for IPv4/IPv6 addressing, system resolution, TCP, UDP, bounded ICMP Echo, and direct-neighbor lookup. The operating system owns the networking stack.

```basic
IMPORT HOST.Net AS Net
```

## External module references

Basic Next provides several provider-backed modules that must be explicitly imported:

- `BNData`: Contract documented in [Appendix H](14_bndata.md).
- `BNWeb`: Added in version 0.3, it provides an HTTP client/server, routes, filters, and URL boundaries. Contract in [Appendix G](13_bnweb.md).
- `BNLog`: Added in version 0.3, it provides structured application and access logging. Contract in [Appendix F](12_bnlog.md).
- `BNJson`: Added in version 0.3, it provides bounded JSON parsing and serialization. Contract in [Appendix E](11_bnjson.md).

## Temporal Data

Basic Next distinguishes strictly between instant-in-time timestamps and human calendar values.

- `TIMESTAMP`: An alias for `INT64` representing an exact moment (milliseconds since the UTC Unix epoch). It supports standard integer arithmetic.
- `DATE`: An immutable Gregorian date (e.g., `2026-08-25`). Logically a 32-bit day count.
- `TIME`: An immutable time of day (e.g., `22:07:20.000`). Logically a 32-bit millisecond count.
- `TIMEZONE`: An IANA identifier (e.g., `America/Sao_Paulo` or `UTC`). The value stores that identifier; the interpreter does not load a TZDB
  database or apply zone rules.

Calendar, formatting, and time-zone operations use explicit calls in the temporal library rather than implicit string conversions.

## Built-ins: `LEN` and `SIZEOF`

Basic Next provides two built-in forms for measuring size. Both evaluate their operand and return an `INTEGER`.

### `LEN(value)`

Returns the logical count of items:
- For a numeric value, the length is `1`.
- For a `STRING`, it is the number of Unicode scalar values (characters).
- For a fixed-size vector, it is the total number of elements across all dimensions.

### `SIZEOF(value)`

Returns the portable byte size of the value's representation, with no padding.
- `BOOLEAN` is 1 byte.
- `DATE` and `TIME` are 4 bytes.
- `STRING` returns the exact UTF-8 byte length of its text.
- Vectors and structs return the sum of their elements' sizes.

`SIZEOF` is a static error for pointers, interfaces, alternative types, and structs containing dynamically sized strings.
