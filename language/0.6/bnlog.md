# BNLog Standard Library 0.3

## Status

Accepted 0.3 delivery scope. Phase 0 freezes the exact typed signatures,
ownership, limits, and error variants before implementation.

`BNLog` is an explicitly imported external standard module. It is inspired by
the architecture of [Winston 3](https://github.com/winstonjs/winston): a logger
separates severity levels, structured entries, formats, and output transports.
BNLog is not a JavaScript binding, does not embed Winston, and introduces no
Node.js runtime dependency. Winston is MIT-licensed reference material.

```basic
IMPORT BNLog AS Log
```

## Dependency direction

The 0.3 dependency graph is acyclic:

```text
HOST.Console ─┐
              ├─ BNLog ─┐
HOST.FileSystem┘         ├─ BNWeb
HOST.Net ────────────────┘
```

`BNWeb` performs all network I/O through the resolved `HOST.Net` provider. It
must not open a second socket stack directly. `BNWeb` uses `BNLog` for access
and internal error records. `BNLog` must not depend on `BNWeb` or `HOST.Net`;
this prevents a logging failure from recursively issuing an HTTP request.

Programs import every module or host capability they use directly. Importing
`BNWeb` does not introduce a `Net` or `Log` alias into application scope.

## Logging model

A logger writes records to one destination in one format. A record
contains:

- timestamp;
- typed severity level;
- message;
- logger label;
- the fields of a `Log.Event`, or of a `Log.Apache` for an access record.

The fixed 0.3 levels preserve Winston's default ordering:

```text
ERROR, WARN, INFO, HTTP, VERBOSE, DEBUG, SILLY
```

Lower numeric priority means higher severity, following the ordering model in
[RFC 5424](https://www.rfc-editor.org/rfc/rfc5424.html). Custom levels are
outside 0.3; fixed typed values avoid dynamic maps and generated methods.
`Log.Level.ERROR` … `Log.Level.SILLY` hold `0` … `6`.

## API (0.6.5)

Decided 2026-10-11 (Carlos; bucket 0.6.5 P.15). It replaces the 0.3
surface (`NEW Logger()`, `AddConsole`, `AddFile`, `AddNull`, `SetFormat`,
`Log`, `Child`, `Fields`, `Entry`, `Transport`), which is removed.

```basic
IMPORT BNLog AS Log
IMPORT HOST.Console AS CON

FUNCTION Start() AS VOID
    LET app AS Log.Logger OR Error = Log.Logger.New("app", "JSON", "CONSOLE")
    IF app IS Error THEN
        RETURN
    END IF
    LET event AS Log.Event
    event.user = "ana"
    LET wrote AS VOID OR Error = app.Info("signed in", event)
    LET closed AS VOID OR Error = app.Close(1000)
END FUNCTION
```

### Making a logger

```basic
Logger.New(label AS STRING, format AS STRING, destination AS STRING) AS Logger OR Error
```

- `label`: 1 to 128 UTF-8 bytes, written in every record.
- `format`: exactly `"JSON"`, `"SYSLOG"` or `"APACHE"` (see Formats).
- `destination`: exactly `"CONSOLE"` (standard output through
  `HOST.Console`), exactly `"NULL"` (discards every record), or otherwise
  the path of a file opened for append through `HOST.FileSystem`. One
  destination per logger; a program that writes to two makes two loggers.

Any other `format`, an empty `destination`, or a `label` out of bounds is
`INVALID_ARGUMENT`; `"CONSOLE"` without `IMPORT HOST.Console`, or a path
without `IMPORT HOST.FileSystem`, is `CAPABILITY_REQUIRED`; a file that
cannot be opened, including one the filesystem policy denies
(`BN_FS_POLICY`), is `IO_FAILED`. `Logger` has no public constructor:
`NEW Log.Logger()` is a static error (`PRIVATE_ACCESS`).

### Records

```basic
EXPORT STRUCT Event
    service AS STRING
    request_id AS STRING
    route AS STRING
    user AS STRING
    detail AS STRING
    code AS INTEGER
END STRUCT

EXPORT STRUCT Apache
    remote AS STRING
    method AS STRING
    path AS STRING
    protocol AS STRING
    status AS INTEGER
    bytes AS INTEGER
    referrer AS STRING
    agent AS STRING
END STRUCT
```

A field that holds its type's zero value (`""`, `0`) is **unset**: it takes
the logger's default for that field (`SetDefaults`), and a field with no
default is absent from the record. An `Event` field is written under its
own name (`request_id`); an `Apache` field under the `http.` name the
APACHE format reads (`remote` → `http.remote`, `bytes` → `http.bytes_sent`,
`agent` → `http.user_agent`; the others keep their name after `http.`).
A field value is at most 4096 UTF-8 bytes; a longer one is
`INVALID_ARGUMENT` and nothing is written.

### Writing

```basic
Logger.Error(message AS STRING, event AS Event) AS VOID OR Error
Logger.Warn(message AS STRING, event AS Event) AS VOID OR Error
Logger.Info(message AS STRING, event AS Event) AS VOID OR Error
Logger.Http(message AS STRING, event AS Event) AS VOID OR Error
Logger.Verbose(message AS STRING, event AS Event) AS VOID OR Error
Logger.Debug(message AS STRING, event AS Event) AS VOID OR Error
Logger.Silly(message AS STRING, event AS Event) AS VOID OR Error
Logger.Access(request AS Apache) AS VOID OR Error
```

Each level method writes one record at its level when the level is at
least as severe as the logger's minimum. `Access` writes one record at
level `HTTP` with an empty message and the `Apache` fields. An APACHE
logger writes only access records: a level method on it is
`INVALID_ARGUMENT`. A JSON or SYSLOG logger writes both kinds.

### Settings

```basic
Logger.SetLevel(minimum AS INTEGER) AS VOID OR Error
Logger.SetDefaults(defaults AS Event) AS VOID OR Error
Logger.SetAccessDefaults(defaults AS Apache) AS VOID OR Error
Logger.Flush(timeoutMs AS INTEGER) AS VOID OR Error
Logger.Close(timeoutMs AS INTEGER) AS VOID OR Error
```

`SetLevel` sets the minimum (when not set: `Log.Level.HTTP` for an APACHE
logger, so its access records are written, and `Log.Level.INFO` otherwise); a value
outside `0..6` is `INVALID_ARGUMENT` and leaves it unchanged.
`SetDefaults` and `SetAccessDefaults` replace the defaults of their record
type with the set fields of `defaults` (an unset field clears that default).
`Flush` writes what the destination buffers; `Close` flushes and closes it,
and is idempotent. A timeout outside `1..60000` ms is `INVALID_ARGUMENT`.
Every call on a closed logger, except `Close`, is `CLOSED`.

Outside 0.6 (BDFL, 2026-10-12): a way to send fields beyond those of
`Log.Event` ("STRUCT desestruturada") is a later proposal.

## Formats

A logger writes every record as one line in the format it was made with
(0.6.5, Carlos 2026-10-09; chosen in `Logger.New` since 2026-10-11).

A record line has no line terminator of its own: the destination writes the
line followed by exactly one line feed. Fields are written in key order,
and a field whose key is sensitive is never written in any format. A key is
read lower-cased, with `-` and `.` read as `_`, and split at `_` into
segments. It is sensitive when one segment, or several consecutive segments
joined by `_`, is `query`, `body`, `passwd` or `tls_key`, or when it
contains `authorization`, `cookie`, `session`, `password`, `secret`,
`token`, `api_key`, `apikey`, `access_key`, `private_key`, `credential`,
`bearer`, `jwt` or `signature` (BDFL, 2026-10-12: the safer reading, so
`request_body`, `passwd_hash` and `token_count` are all withheld).

- **`"JSON"` (JSON Lines):** one UTF-8 JSON object,
  `{"fields":{…},"label":…,"level":…,"message":…,"timestamp":…}`, members in
  that order; `level` is the level name (`"INFO"`); every string is a JSON
  string ([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) escapes).
- **`"SYSLOG"`** ([RFC 5424](https://www.rfc-editor.org/rfc/rfc5424)):
  `<PRI>1 TIMESTAMP HOSTNAME APP-NAME PROCID - - MSG`. `PRI` is facility
  `user` (1) times 8 plus the level's severity: `ERROR` 3, `WARN` 4,
  `INFO` and `HTTP` 6, `VERBOSE`, `DEBUG` and `SILLY` 7 (so `INFO` is
  `<14>`). `TIMESTAMP` is the record time, `HOSTNAME` the machine's host
  name, `APP-NAME` the logger label, `PROCID` the process identifier; a
  header field that is empty or not printable US-ASCII without spaces (or
  longer than 255 bytes for `HOSTNAME`, 48 for `APP-NAME`) is `-`. There
  is no `MSGID` and no structured data. `MSG` is the message followed by
  ` key=value` for each field; line feed, carriage return and tab become a
  space and any other control character becomes `?`, so one record is one
  line.
- **`"APACHE"` (Apache Combined Log Format):**
  `remote - - [time] "request" status bytes "referrer" "user-agent"`, read
  from the fields `http.remote`, `http.method` + `http.path` +
  `http.protocol` (joined by a space, absent parts left out),
  `http.status`, `http.bytes_sent`, `http.referrer` and `http.user_agent`; an
  absent field is `-`. `time` is the record time as `dd/Mon/yyyy:HH:MM:SS
  +0000`. The query of `http.path` and `http.referrer` is not written. Inside
  quotes, `"` is written `\"`, `\` is `\\`, and a control character is
  `\xhh`; outside quotes, a control character is `?`. The label, message
  and other fields are not written.

A record whose line would exceed 65,536 bytes is `LIMIT`; nothing is
written. Timestamp, label and field-inclusion options, console color,
formatter callbacks, templates and mutation pipelines are outside 0.6.5.

## Destinations

The destinations are:

- **`"CONSOLE"`:** standard output through `HOST.Console`;
- **a file path:** append-only UTF-8 through `HOST.FileSystem`, flushed by
  `Flush` and `Close`;
- **`"NULL"`:** deterministic discard, for disabling an output without
  branching application code.

A write is synchronous: the call returns after the destination accepted
the line, or with its error. `Flush` and idempotent `Close` report errors
and are required before normal shutdown.

Queues, background workers, arbitrary streams, database transports, HTTP
transports, log querying, profiling, custom transports, and built-in file
rotation are outside 0.3. Production deployments should normally write JSON
Lines to stdout and let the operating system or container platform collect,
rotate, and forward records.

## `BNWeb` integration

`BNWeb` accepts an explicitly configured logger. It emits structured `HTTP`
entries for completed or rejected exchanges and `ERROR` entries for internal
failures. Each access entry includes, when available:

- request ID;
- method, normalized target, route name, and protocol version;
- transport peer and trusted effective client address as distinct fields;
- status, duration, bytes received, and bytes sent;
- user agent and referrer after control-character escaping.

BNWeb never logs request or response bodies, authorization, cookies, session
identifiers, TLS private material, or query values by default. Explicit
application logging remains subject to the same field and record-size limits.
`BNWeb` takes a `Log.Logger` (`Server.New(logger)`, `Client.New(logger)`); its
access records are the `Access` records of that logger, and its internal
failures are `Error` records.
Logging failure is observable but must not change a response that has already
been committed.

## Errors

`Code` of a BNLog `Error` ([error.md](error.md)) is one of these
`INTEGER` constants of the module (`Log.INVALID_ARGUMENT` under
`IMPORT BNLog AS Log`). `Operation` names the member (`BNLog.Logger.Info`).

| Constant | Value | When |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | An unknown format or level; an empty destination; a label or field value past its bound; a timeout outside `1..60000` ms; a level method on an APACHE logger |
| `DUPLICATE` | 2 | Reserved: no 0.6.5 member returns it (the 0.3 `Fields` did) |
| `LIMIT` | 3 | A record line past 65,536 bytes |
| `CLOSED` | 4 | An operation on a closed logger |
| `IO_FAILED` | 5 | The destination could not be opened, written or flushed, including a file the filesystem policy denies (`BN_FS_POLICY`) |
| `CAPABILITY_REQUIRED` | 6 | A file destination without `IMPORT HOST.FileSystem`, `"CONSOLE"` without `IMPORT HOST.Console` (programs import every capability they use) |
| `TIMEOUT` | 7 | `Flush` or `Close` ran past its timeout |
| `UNAVAILABLE` | 8 | The operation or provider is not available |

## Required limits and verification

The bounds are: label 1..128 bytes, field value 4096 bytes, record line
65,536 bytes, flush/close timeout 1..60000 ms. Tests cover every format of
both record kinds, level filtering, defaults, unset fields, escaping, JSON
validity, destinations (console, file append, null), flush/close, the
refusals of the table above, secret exclusion, and BNWeb access-record
mapping, on both backends. No test requires a public logging service.
