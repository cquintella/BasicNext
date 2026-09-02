# Appendix G: BNWeb

`BNWeb` is the external standard module for web communication. It provides HTTP client and server adapters, routing, filtering, and a shared request/response pipeline.

Because `BNWeb` is an external module, you must import it explicitly:

```basic
IMPORT BNWeb AS Web
```

## Architecture and Dependencies

`BNWeb` is built on top of `HOST.Net` for all socket and DNS resolution operations. It does not create its own network capability. Furthermore, it integrates with `BNLog` for structured access logging.

You must import capabilities directly; importing `BNWeb` does not expose `Net` or `Log` implicitly:

```basic
IMPORT HOST.Net AS Net
IMPORT BNLog AS Log
IMPORT BNWeb AS Web
```

## URL Boundary and Safety

`BNWeb` follows a strict URL boundary to prevent security flaws like path traversal and request smuggling:
1. Parse -> Validate -> Canonicalize -> Route Match -> Ordered Filters -> Handler
2. Invalid or ambiguous URLs are rejected immediately with `400 Bad Request`.
3. Encoded path separators (`%2F`, `%5C`) and `.`/`..` segments are rejected.

The framework ensures the original malformed string is never executed as a route or filesystem path.

## Protocol Scope and 0.4 Roadmap

Version 0.3 requires **HTTP/1.1**, **HTTP/2**, TLS, and ALPN.

To manage expectations and scope, the following advanced features are explicitly deferred to **version 0.4**:
* **HTTP/3 and QUIC:** Not supported in the 0.3 pipeline.
* **Concurrent Transport Callbacks:** `BNWeb` transport-to-BN threading relies on sequential bounds for now; concurrent dispatch integration is scheduled for 0.4.
* **HTTPS Client Trust-Root Management:** Advanced client-side certificate authority configurations.
* **Transport Access-Log Integration:** Deeply integrated native transport logging.
