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
selection is ordered and deterministic. Phase 0 must choose and document
whether literal specificity precedes registration order when two patterns
match the same method and path; implementation must not invent that rule.

Parameter names are unique within a pattern. Matching uses the canonical
decoded path produced by the URL boundary. Query fields never select a route.
No match produces `404`; a path match with the wrong method produces `405` and
an `Allow` header. `HEAD` and `OPTIONS` behavior must be frozen in Phase 0.
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

Duplicate-header policy, request-body consumption, streaming/file behavior,
form decoding, and exact collection signatures are Phase 0 decisions. No
handler receives an unbounded body by default.

## Server lifecycle

BN handlers are synchronous and run serially in 0.3. An internal bounded
`HOST.Net` runtime may perform concurrent transport I/O, but it does not add
`ASYNC`/`AWAIT` syntax or concurrent handler execution. The explicit `Server.Dispatch`
operation invokes BN filters and route handlers; HTTP transport adapters use
the approved Rust callback bridge in the verified Sprint 11 boundary.
Transport-to-BN callback projection is gated for Sprint 13 `BNDispatch`. Start, dispatch, overload, graceful
stop, cleanup, and idempotent close have explicit time and queue bounds.

The 0.4 design may add opt-in asynchronous named functions scheduled on an
explicit `BNDispatch` queue and awaited through bounded tickets. That design is
not part of this 0.3 grammar or support claim; see the [0.4 async/await
design](../../superpowers/specs/2026-09-01-async-await-0.4-design.md).

## Client and security policy

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
