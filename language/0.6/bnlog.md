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

0.3 provides three bounded formats:

- **JSON Lines:** one UTF-8 JSON object per record, suitable for production
  stdout collection;
- **Text:** timestamp, label, level, message, and escaped fields on one line;
- **Apache Combined:** access records supplied by `BNWeb`, with control
  characters escaped before output.

Timestamp, label, field inclusion, color, and final rendering are explicit
format options. Color is valid only for an interactive console and never
appears in JSON or file output. Arbitrary formatter callbacks, template
languages, and mutation pipelines are outside 0.3.

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

## Required limits and verification

Phase 0 defines maximum transports per logger, fields per entry, key/message
length, serialized record size, child depth, and flush/close timeout. Tests
cover level filtering, transport order, multiple simultaneous outputs,
escaping, JSON validity, typed fields, child context, partial transport
failure, flush/close, file append, secret exclusion, and BNWeb access-record
mapping. No test requires a public logging service.
