# Basic Next Error contract

`Error` is a standard-library object, not a keyword. A program receives one
from a fallible operation (`T OR Error`); it cannot construct one
(`NEW Error` is not available). A user class must not `EXTENDS Error`.

## Fields (0.6.2)

| Field | Type | Contents |
| --- | --- | --- |
| `Code` | `INTEGER` | A portable constant of the capability or module that owns the operation. |
| `Operation` | `STRING` | The canonical qualified name of the failing operation, independent of import aliases (`HOST.FileSystem.Open`). |
| `Message` | `STRING` | What failed, naming the inputs that identify the failure. Never empty. |
| `Cause` | `STRING` | Why it failed: the violated rule, the policy that denied it, or the operating-system error. Never empty. |

The fields are immutable. `Code`, `Operation`, and `Message` are identical on
every backend and host for the same failure; `Cause` may carry
operating-system text, which differs between hosts.

`PRINT` of an `Error` writes
`Error <Code> in <Operation>: <Message> (cause: <Cause>)`.

A capability or module publishes its codes as `INTEGER` constants
(`FS.NOT_FOUND`, `Net.TIMEOUT`); values are stable and distinct within their
owner. An operating-system error number may appear in `Cause`, never in
`Code`. Code tables: `HOST.FileSystem` in [host.md](host.md), `HOST.Net` in
[host-net.md](host-net.md), `HOST.Exec` in [0.6.md](0.6.md#hostexec-051).

TODO (normative gap): code tables for `BNData`, `BNJson`, `BNWeb`, `BNLog`,
`BNCrypto`, `BNDispatch`, `BNMath`, and `BNString`.

## Runtime diagnostics are not `Error` values

In 0.1, `TryParse` operations return their documented value type or `Error`
when failure is an expected result. `Parse` operations require success and
raise the runtime error `PARSE_ERROR` when parsing fails.

In 0.2, `Float.TryParse` is withdrawn. Numeric text conversion is
`BNMath.VAL`, which always returns `FLOAT` and does not use `Error`.
Temporal `Parse` operations still raise `PARSE_ERROR`. File and CSV
operations return `T OR Error`; they do not raise exceptions. Unterminated
CSV quotes, `WriteCSV` I/O failure, and `WriteBytes` I/O failure are
`Error` values, not runtime diagnostics. `ReadBytes` I/O, a closed file,
and a text-family file are `Error` on `INTEGER OR EOF OR Error`.
