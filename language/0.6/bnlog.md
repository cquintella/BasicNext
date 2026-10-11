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

A logger owns an ordered set of transports. A log entry contains:

- timestamp;
- typed severity level;
- message;
- logger label;
- optional structured fields;
- optional source location and `Error` information when supplied explicitly.

The fixed 0.3 levels preserve Winston's default ordering:

```text
ERROR, WARN, INFO, HTTP, VERBOSE, DEBUG, SILLY
```

Lower numeric priority means higher severity, following the ordering model in
[RFC 5424](https://www.rfc-editor.org/rfc/rfc5424.html). Custom levels are
outside 0.3; fixed typed values avoid dynamic maps and generated methods.

Structured fields use an opaque bounded collection with typed setters and
`Count`/`Get` access. They are not a variable-size BN vector and must reject
duplicate keys. A child logger may add immutable default fields such as
`service`, `request_id`, or `route` while sharing the parent's transport
configuration.

## Formats

A logger writes every record as one line in one of three formats (0.6.5,
Carlos 2026-10-09), chosen by name:

```basic
Logger.SetFormat(format AS STRING) AS VOID OR Error
```

`format` is exactly `"JSON"` (the default), `"SYSLOG"` or `"APACHE"`. Any
other text is `INVALID_ARGUMENT` and leaves the format unchanged; on a
closed logger the call is `CLOSED`. A `Child` starts with its parent's
format; a later `SetFormat` on either does not change the other. The format
applies to every transport of the logger.

A record line has no line terminator of its own: each transport writes the
line followed by exactly one line feed. Fields are written in key order,
and a field whose key is sensitive is never written in any format. A key is
sensitive when, lower-cased with `-` and `.` read as `_`, it is `query`,
`body`, `passwd` or `tls_key`, or it contains `authorization`, `cookie`,
`session`, `password`, `secret`, `token`, `api_key`, `apikey`,
`access_key`, `private_key`, `credential`, `bearer`, `jwt` or `signature`.
TODO (normative gap, Carlos decides): `request_body` and `passwd_hash` are
written, while `token_count` is not.

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

## Transports

The 0.3 core transports are:

- **Console:** stdout/stderr through `HOST.Console`, with a per-transport
  minimum level;
- **File:** append-only UTF-8 through `HOST.FileSystem`, with a per-transport
  minimum level and explicit flush;
- **Null:** deterministic discard transport for disabling an output without
  branching application code.

Dispatch is synchronous and serial in transport-registration order. Every
eligible transport is attempted. The operation returns the first transport
error after all eligible transports have been attempted. `Flush` and
idempotent `Close` report errors and are required before normal shutdown.

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
Logging failure is observable but must not change a response that has already
been committed.

The native `Fields` and `Entry` providers enforce the field boundary: at most
64 fields, non-empty keys up to 128 UTF-8 bytes, and entry field values up to
4096 UTF-8 bytes. Duplicate keys are rejected without mutating the original
value.

## Errors

`Code` of a BNLog `Error` ([error.md](error.md)) is one of these
`INTEGER` constants of the module (`Log.INVALID_ARGUMENT` under
`IMPORT BNLog AS Log`). `Operation` names the member (`BNLog.Logger.Log`).

| Constant | Value | When |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | An unknown level; an invalid transport; a label, message, field key, or field value past its bound |
| `DUPLICATE` | 2 | A field or entry key that already exists |
| `LIMIT` | 3 | Too many transports, fields, or entry fields |
| `CLOSED` | 4 | An operation on a closed logger |
| `IO_FAILED` | 5 | A transport could not write or flush, including a file the filesystem policy denies (`BN_FS_POLICY`) |
| `CAPABILITY_REQUIRED` | 6 | `AddFile` without `IMPORT HOST.FileSystem`, `AddConsole` without `IMPORT HOST.Console` (programs import every capability they use) |
| `TIMEOUT` | 7 | `Flush` or `Close` ran past its timeout |
| `UNAVAILABLE` | 8 | The operation or provider is not available |

## Required limits and verification

Phase 0 defines maximum transports per logger, fields per entry, key/message
length, serialized record size, child depth, and flush/close timeout. Tests
cover level filtering, transport order, multiple simultaneous outputs,
escaping, JSON validity, typed fields, child context, partial transport
failure, flush/close, file append, secret exclusion, and BNWeb access-record
mapping. No test requires a public logging service.
