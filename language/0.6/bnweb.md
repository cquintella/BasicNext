# BNWeb Standard Library 0.3

## Status

Accepted 0.3 functional scope. Phase 0 freezes exact typed signatures,
ownership, errors, limits, and lifecycle before implementation.

`BNWeb` is an explicitly imported external standard module:

```basic
IMPORT HOST.Net AS Net
IMPORT BNLog AS Log
IMPORT BNWeb AS Web
```

Every resolver and socket operation is supplied by the resolved `HOST.Net`
provider. `BNWeb` owns HTTP policy and typed request/response values, not a
second network capability. Its logging integration uses an explicitly supplied
`BNLog` logger and never creates a hidden global logger.

## Protocol scope

0.3 requires HTTP/1.1, HTTP/2, TLS, and ALPN. Client and server adapters share
one request, response, routing, filtering, provenance, limit, and logging
pipeline. QUIC and HTTP/3 are deferred to 0.4 and are absent from the 0.3
dependency and conformance graphs.

## URL boundary

BNWeb does not sanitize a malformed URL into a different valid URL. The fixed
boundary is:

```text
parse -> validate -> canonicalize -> route match -> ordered filters -> handler
```

Invalid or ambiguous input is rejected, normally with `400 Bad Request` on the
server or a typed client error. The original string is never executed as a
route or filesystem path.

For inbound request targets, BNWeb:

- enforces the raw target and component limits before allocation;
- rejects NUL, ASCII control characters, raw backslashes, malformed percent
  escapes, and text that is not valid UTF-8 after decoding;
- splits path and query before decoding;
- percent-decodes each component exactly once;
- rejects encoded path separators (`%2F` and `%5C`, case-insensitive) and
  decoded `.` or `..` path segments;
- preserves repeated slashes and treats a trailing slash as significant;
- excludes the query from route matching;
- preserves duplicate query fields in source order;
- leaves `+` as plus in a generic URL query; form decoding is a separate
  content-type operation.

For absolute client URLs, BNWeb accepts only `http` and `https`, requires a
valid authority, rejects user information, canonicalizes scheme/host/default
port through the approved URL implementation, and never sends a fragment.
SSRF policy is applied after every resolution and redirect, for every returned
IPv4/IPv6 address.

## Routes

Routes are registered explicitly for an HTTP method and path pattern. 0.3
patterns contain literal segments and `:name` parameter segments only. Route
selection is ordered and deterministic: when two patterns match the same method
and path, literal specificity (more literal segments) precedes registration
order, and registration order breaks ties.

Parameter names are unique within a pattern. Matching uses the canonical
decoded path produced by the URL boundary. Query fields never select a route.
No match produces `404`; a path match with an unregistered method produces `405`
and an `Allow` header listing permitted methods in sorted order. An incoming `HEAD`
request matches an explicit `HEAD` route first or falls back to a matching `GET`
route and suppresses the response body on emission; an incoming `OPTIONS` request
matches an explicit `OPTIONS` route first or automatically returns `204 No Content`
with the `Allow` header containing all methods registered for that path pattern.
Regex routes, implicit controller discovery, and generic middleware/plugins are
outside 0.3.

## Filters

Filters are ordered, typed request-policy steps. They run after URL
canonicalization and route selection but before the handler. A filter may:

- continue without changing the canonical route identity;
- add bounded typed context for later filters and the handler;
- reject with an explicit response;
- fail with `Error`, which follows the server failure policy.

Filters do not rewrite malformed URLs, re-run route matching, execute hidden
network requests, or mutate committed responses. Built-in policy covers
request/URL limits, trusted-proxy provenance, ACL, and rate/overload decisions.
Application filters use ordinary typed function values; there is no second
plugin or middleware activation model.

## Request and response

Requests expose typed method, canonical URL components, headers, peer/effective
origin, route parameters, query values, cookies, and bounded body consumption.
Responses expose explicit status, headers, bounded body writes, commit state,
and close behavior. Header names/values reject protocol-invalid controls; a
committed response cannot be reset or silently replaced.

### Route parameters, query values and cookies (0.6.5)

Bucket 0.6.5 E.S0.d (Carlos, 2026-10-10). A route `GET /users/:id` matches
`/users/00102`; the handler reads `request.Param("id")`, `"00102"`. A query
field never selects a route: `GET /users` serves `/users?id=00102`, and
`request.Query("id")` returns its values.

| Member | Result |
| --- | --- |
| `Request.Param(name AS STRING) AS STRING OR Error` | The `:name` segment of the matched pattern; `NOT_FOUND` when the pattern has no such parameter |
| `Request.Cookie(name AS STRING) AS STRING OR Error` | The value of the cookie `name` the client sent ([RFC 6265](https://www.rfc-editor.org/rfc/rfc6265) §5.4: `name=value` pairs separated by `;` in every `Cookie` header, a quoted value without its quotes, the first pair with that name); `NOT_FOUND` when absent |
| `Response.SetCookie(name AS STRING, value AS STRING, domain AS STRING, path AS STRING, maxAgeMs AS INTEGER) AS VOID OR Error` | Adds a `Set-Cookie` header with the default policy: `Secure`, `HttpOnly`, `SameSite=Lax` |
| `Response.SetCookieWithPolicy(name AS STRING, value AS STRING, domain AS STRING, path AS STRING, maxAgeMs AS INTEGER, secure AS BOOLEAN, httpOnly AS BOOLEAN, sameSite AS STRING) AS VOID OR Error` | The same with an explicit policy, as `CookieJar.SetWithPolicy` |
| `Response.ClearCookie(name AS STRING, domain AS STRING, path AS STRING) AS VOID OR Error` | Expires the cookie (`Max-Age=0`) |

An empty `domain` is a host-only cookie; `maxAgeMs` `0` is a session cookie
(no `Max-Age`), a negative one is `INVALID_ARGUMENT`; `Max-Age` is written in
whole seconds (rounded down). `sameSite` is
`"Strict"`, `"Lax"` or `"None"`, and `"None"` needs `secure`
(`INVALID_ARGUMENT` otherwise). A `Secure` cookie is set only on a response
to a request that arrived over TLS: on cleartext HTTP it is
`INVALID_ARGUMENT`, so a program served over `http://` passes `secure` as
`FALSE` explicitly. `ClearCookie` marks the cookie `Secure` only over TLS.
Cookie names and values are bounded and reject protocol-invalid characters
(`INVALID_ARGUMENT`).

Duplicate headers are preserved in arrival order and accessed via `Request.Header(name)`
returning `HeaderValues OR Error` (`Count() AS INTEGER`, `Get(index AS INTEGER) AS STRING OR Error`).
Request bodies are read with an explicit maximum via `Request.Body(maximum AS INTEGER) AS STRING OR Error`
(up to the 8 MiB protocol ceiling); incoming payloads that exceed 8 MiB at the transport boundary
are rejected with status `413 Payload Too Large`. No handler receives an unbounded body.

## Server lifecycle

BN handlers are synchronous and, without `Server.UseQueue` (below), run
serially. An internal bounded
`HOST.Net` runtime may perform concurrent transport I/O, but it does not add
`ASYNC`/`AWAIT` syntax or concurrent handler execution. The explicit `Server.Dispatch`
operation invokes BN filters and route handlers; HTTP transport adapters use
the approved Rust callback bridge in the verified Sprint 11 boundary.
Transport-to-BN callback projection is gated for Sprint 13 `BNDispatch`. Start, dispatch, overload, graceful
stop, cleanup, and idempotent close have explicit time and queue bounds.

### Parallel handlers and server sessions (0.6.5)

Bucket 0.6.5 E.S0.e (Carlos, 2026-10-10). A server runs its handlers in
parallel on a `BNDispatch` queue the program owns:

```basic
IMPORT BNDispatch AS Dispatch
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(8)
server.UseQueue(queue)
server.UseSessions(1024, 900000)
LET started AS VOID OR Error = server.Start(endpoint)
```

| Member | Result |
| --- | --- |
| `Server.UseQueue(queue AS Dispatch.Queue) AS VOID OR Error` | Every request's filters and handler run as one task of `queue`: at most its workers at once |
| `Server.UseSessions(capacity AS INTEGER, idleMs AS INTEGER) AS VOID OR Error` | The server keeps at most `capacity` sessions, each forgotten after `idleMs` without a request |
| `Request.Session() AS Session OR Error` | The session of this client: the one its `sid` cookie names, or a new one |
| `Session.Id() AS STRING` | The session identifier |
| `Session.Get(key AS STRING) AS STRING OR Error` | The value stored under `key`; `NOT_FOUND` when absent |
| `Session.Set(key AS STRING, value AS STRING) AS VOID OR Error` | Stores `value` under `key` |
| `Session.Remove(key AS STRING) AS VOID OR Error` | Removes `key`; `NOT_FOUND` when absent |
| `Session.Rotate() AS VOID OR Error` | Gives the session a new identifier, keeping its values (call it on login) |
| `Session.End() AS VOID OR Error` | Forgets the session (call it on logout) |

`UseQueue` and `UseSessions` are called before a `Start` member; on a
started server they are `INVALID_ARGUMENT`. A handler task runs in an
isolated copy of the program, as an `ASYNC` worker does (0.6.md, "Isolation
and conformance"): handlers share no BN variable or object, and the
sessions are the state they share. Without `UseQueue`, handlers run as
before. A queue with 1024 pending tasks answers `503`; `Stop` stops
accepting, then waits within its timeout for the running tasks. Closing
the queue while the server runs fails the waiting requests with `503`.

Sessions are kept by the server and shared by every task. `Session()`
without `UseSessions` is `UNAVAILABLE`. The server writes the session
cookie itself after the handler: `sid`, `Path=/`, `HttpOnly`,
`SameSite=Lax`, and `Secure` when the request arrived over TLS. A new or
rotated session sets the cookie; `End` clears it. Identifiers come from the
system's entropy (128 bits). Bounds: a key has 1 to 128 bytes, a value at
most 4,096, a session at most 64 keys (`LIMIT` past them); a full server
forgets the session idle the longest before creating a new one.

`Id`, `Set` and `Rotate` create the session when the request has none;
`Get` and `Remove` never do, so a visitor who only reads gets no cookie.
`capacity` is 1 to 10,000 and `idleMs` 1 to 1,800,000 (30 minutes);
outside them `UseSessions` is `INVALID_ARGUMENT`. A `Session` built with
`NEW` reaches no server: its members are `UNAVAILABLE`. The in-process
`Server.Dispatch` runs the handler in the caller, never on the queue, and
keeps sessions as the network server does.

The 0.4 design may add opt-in asynchronous named functions scheduled on an
explicit `BNDispatch` queue and awaited through bounded tickets. That design is
not part of this 0.3 grammar or support claim; it is described in the internal
0.4 async/await design.

## Client and security policy

`Client.New(logger AS Log.Logger)` keeps that logger for the client
(BDFL decision, 2026-10-17). Each `Client.Request` and
`Client.RequestWithPolicy` writes one access record through it, at level
HTTP, in the logger's format ([bnlog.md](bnlog.md), "Formats"). The record
carries the method, the path of the URL the call names with the query
removed, the protocol `HTTP/1.1`, the status, and the response body size.
A transport failure records status `0`. The response time in milliseconds
is the field `http.duration_ms`. Response headers are fields
`http.header.<name>` with the name in lower case; a header whose key is
sensitive is omitted. The query, the body, userinfo, and TLS private
material are not written. `NEW Client()` has no logger and writes nothing.

The 0.3 client has bounded cleartext connect/read/write/total timeouts,
response bodies, decompression policy, cookie jars, and connection reuse.
HTTPS client transport and certificate trust roots are gated for Sprint 13 by
D-026; 0.3 makes no HTTPS client support claim before that gate closes. HTTPS servers
never silently downgrade and retain the accepted certificate/SNI/ALPN policy.

HTTPS servers receive certificates through the explicit external `TLSConfig`
type. `TLSConfig.FromPEM(certificatePem, privateKeyPem)` accepts bounded PEM
strings, and `Server.StartTLS(endpoint, config)` starts the synchronous handler
pipeline over Rustls with HTTP/1.1 and HTTP/2 ALPN. Private key material is
provider-owned for the configuration lifetime and excluded from logs.

## State, audit, and static parsing

0.3 includes isolated cookie jars, bounded in-memory sessions, ordered ACL,
static non-executing HTML parsing, and access/error records through `BNLog`.
Geolocation is not part of the 0.3 support claim and no MMDB dataset is bundled
or downloaded. Scripts, event handlers, subresources,
and browser automation never execute. Bodies, authorization, cookies, session
identifiers, query values, and TLS private material are not logged by default.

The public state surface is explicit: `CookieJar` provides bounded set/get/
delete operations; `SessionStore` provides bounded create/get/set/delete with
idle expiry; `Scraper.Parse` and `Text(selector)` perform static extraction;
`ACL` stores ordered allow/deny CIDRs and checks an address. These types hold
no hidden network or filesystem capability.

Templates, WebSocket, ORM, OAuth, persistent/distributed sessions, browser
automation, and HTTP/3 are outside 0.3.

## Verification boundary

Conformance uses local clients, servers, DNS/resolver doubles, and certificate
fixtures. Tests cover canonicalization, ambiguous URL rejection, routes,
filters, response commit, HTTP/1.1/2 parity, TLS, limits, timeouts, SSRF,
sessions, ACL, logging, static non-execution, stop, and cleanup without a public
Internet dependency.

## Errors

Every function and method in `BNWeb` that returns `T OR Error` yields an
`Error` value on failure with `Code`, `Operation`, `Message`, and `Cause`.
`Code` is one of:

| Constant | Value | When |
| --- | ---: | --- |
| `Web.INVALID_ARGUMENT` | 1 | A status outside 100..599; an unsupported method; a negative cookie age; an invalid SameSite policy or `SameSite=None` without `Secure`; an invalid session idle timeout; a route handler or filter that is not a FUNCTION |
| `Web.NOT_FOUND` | 2 | A cookie, session, header, or route that does not exist |
| `Web.OUT_OF_RANGE` | 3 | An index outside a collection |
| `Web.LIMIT` | 4 | Too many filters, handlers, or connections |
| `Web.CLOSED` | 5 | The server is not accepting requests (stopped or not started) |
| `Web.TIMEOUT` | 6 | A request, response, or egress call ran past its deadline |
| `Web.EGRESS_DENIED` | 7 | The egress policy denies the destination or the redirect |
| `Web.HTTP_FAILED` | 8 | An outbound request failed (connection, TLS, protocol); `Cause` carries the Net or TLS reason |
| `Web.UNAVAILABLE` | 9 | The provider (TLS, session store, scraper, …) is not in this build |
