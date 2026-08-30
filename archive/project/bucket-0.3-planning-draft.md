# Basic Next 0.3 Planning Draft (Archived)

This incomplete draft was archived during the 0.2 release reconstruction.
Its referenced 0.3 normative documents were not present in the recreated
repository, so it is not an active or accepted delivery plan.

The archived 0.2 work program is
[`bucket-0.2.md`](../archive/project/bucket-0.2.md). The implementation plan is
[`WBS-0.3.md`](WBS-0.3.md).

Authority, in order: [`0.3.ebnf`](../docs/language/0.3/0.3.ebnf),
[`0.3.md`](../docs/language/0.3/0.3.md), and
[`keywords.md`](../docs/language/0.3/keywords.md).

## Accepted delivery boundary

0.3 delivers native-interpreter IPv4/IPv6 networking through
`HOST.Network` and the explicitly imported `BNWeb` module. The operating
system owns IP, TCP, UDP, routing, congestion control, DNS/system resolution,
and neighbor tables. Basic Next does not implement a network stack, TLS,
cryptography, QUIC, HTTP framing, cookie grammar, or HTML parsing itself.

Mandatory 0.3:

- typed IPv4/IPv6 addresses, CIDRs, endpoints, forward/reverse system lookup,
  and direct ARP/NDP neighbor lookup;
- outbound TCP plus TCP/UDP bind and bounded I/O through `HOST.Network`;
- `BNWeb` HTTP/1.1 and HTTP/2 client/server, HTTPS, deterministic routes,
  limits, filtering, trusted-proxy provenance, ACL, local geolocation, and
  Apache access logs;
- isolated client cookies, bounded in-memory server sessions, and static HTML
  scraping without JavaScript execution;
- QUIC v1 and HTTP/3 after a separate dependency and interoperability gate;
- deterministic rejection on unsupported compiler, Jupyter, and wasm hosts.

Explicitly outside `BNWeb` 0.3: template engines, WebSocket, ORM, OAuth,
generic plugins, generic middleware, browser automation, persistent or
distributed sessions, and full DNS-record APIs.

Optional and not on the release critical path: `BNNet` and static-file serving.
They need their own accepted decision before implementation.

## Implementation strategy

```text
BN program
  → typed HOST.Network / BNWeb values
  → semantic identity + typed BN IR
  → native interpreter providers
      → HOST.Network → Rust stdlib / operating-system sockets and resolver
      → BNWeb HTTP   → approved HTTP/TLS/QUIC/HTML libraries
```

Use the existing `BNMath`/`BNData` resolved-module identity pattern for
`BNWeb`. Do not dispatch merely because a module or class has a familiar name.
All network reads, bodies, queues, redirects, decompression, connections, and
timeouts are bounded.

The reference interpreter keeps BN handlers synchronous and serial behind a
bounded request queue. The I/O backend may be asynchronous, but 0.3 adds no
async syntax and makes no thread-safety claim for the interpreter. Add parallel
handler execution only after a separate runtime design and measurements.

HTTP/1.1 and HTTP/2 establish the shared request/response pipeline before
HTTP/3. HTTP/3 is the final transport adapter, not a separate router, security,
session, ACL, or logging implementation.

## Sprint exit criteria

A sprint closes only when all its checkboxes are complete and:

- accepted signatures agree across specification, keywords, semantics, IR,
  and module declarations;
- positive and negative fixtures cover the new surface with source spans;
- no live Internet service is required by the test suite;
- network/protocol tests use local IPv4 and IPv6 endpoints where supported;
- `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and
  `git diff --check` pass for Rust changes;
- dependency changes include exact versions, licenses, minimal enabled
  features, security review, and BDFL approval.

Do not start a later sprint while an earlier sprint is open, except inherited
gate `G0.1`, which blocks release but not 0.3 implementation.

## G0 — Inherited release gate

### G0.1 — Real Windows TTY evidence from 0.2

- [ ] Run `tests/windows-console-evidence.ps1` in Windows Terminal or
      `conhost`.
- [ ] Record resize, bounds, `PrintAt`, and piped-stdin traces.
- [ ] Update the archived 0.2 evidence and close WBS Activity 0.2.1.

## Sprint 0 — Decisions and normative API freeze

WBS: Phase 0 and Phase 1.

- [ ] Approve the required/optional/out-of-scope table.
- [ ] Spike and approve the HTTP/TLS/QUIC/runtime dependency set; do not add
      production dependencies before the decision record exists.
- [ ] Freeze the synchronous-handler, bounded-queue, serve/stop/delete model.
- [ ] Add `HOST.Network.ConnectTCP`; specify partial I/O, EOF, timeout, and
      ownership for every transport object.
- [ ] Define system resolver semantics rather than raw DNS-record promises.
- [ ] Freeze native interpreter support and deterministic rejection for native
      compiler, Jupyter, and wasm until their providers exist.
- [ ] Freeze complete `Request`, `Response`, routes, limits, and lifecycle APIs.
- [ ] Reconcile `0.3.md`, `0.3.ebnf`, `keywords.md`, and `WBS-0.3.md`.

**Exit:** no public 0.3 method has an unspecified type, ownership rule, limit,
error channel, or host-availability outcome.

## Sprint 1 — Address and CIDR foundation

WBS: Phase 2.

- [ ] Reuse `std::net` for address parsing, display, and endpoints.
- [ ] Implement only the missing CIDR network/mask/containment logic.
- [ ] Add host-owned network identities to semantics and typed BN IR.
- [ ] Add import, type, identity, IPv4/IPv6, bounds, and target-rejection tests.

**Exit:** pure values work without system I/O and cannot be forged by user
classes.

## Sprint 2 — System resolver, TCP, and UDP

WBS: Phase 3, excluding deferred Activity 3.3.2.

- [ ] Implement injected forward/reverse system resolver providers.
- [ ] Implement TCP connect, bind, listen, accept, bounded read/write, timeout,
      EOF, endpoints, and idempotent close.
- [ ] Compose explicit IPv4 and IPv6 listeners with all-or-nothing startup.
- [ ] Implement bounded UDP send/receive without silent truncation.
- [ ] Implement safe per-platform ARP/NDP neighbor lookup for direct L2 peers.
- [ ] Pass local IPv4/IPv6 loopback and negative error-path tests.

**Exit:** `HOST.Network` is sufficient for an HTTP client, an HTTP server, and
the later QUIC adapter without exposing descriptors or raw packets.

## Sprint 3 — BNWeb provider and HTTP application model

WBS: Activities 4.1.1 and 4.2.1–4.2.3; HTTP transport work remains Sprint 4.

- [ ] Add typed `modules/bn/BNWeb.bn` declarations and resolved module identity.
- [ ] Implement the transport-neutral request/response representation.
- [ ] Implement mandatory filtering and trusted-proxy provenance.
- [ ] Implement ordered literal and `:name` routes.
- [ ] Implement response commit rules, automatic `404`/`405`, and handler
      failure mapping.
- [ ] Implement bounded queues, timeouts, overload behavior, graceful stop,
      and cleanup.

**Exit:** the application pipeline is deterministic and transport-independent;
no generic middleware or plugin mechanism exists.

## Sprint 4 — HTTP/1.1 and HTTP/2 baseline

WBS: Activity 4.1.2.

- [ ] Integrate the approved HTTP library; do not write a private framing
      parser.
- [ ] Map HTTP/1.1 and HTTP/2 into the shared application pipeline.
- [ ] Cover keep-alive, multiplexing boundaries, malformed framing, authority,
      header/body limits, cancellation, and slow clients.
- [ ] Demonstrate ordinary local clients receive identical route semantics over
      HTTP/1.1 and HTTP/2.

**Exit:** cleartext local protocol tests pass. Public deployment still waits
for Sprint 5 HTTPS.

## Sprint 5 — HTTPS and bounded HTTP client

WBS: Phase 5.

- [ ] Integrate approved TLS with ALPN `h2` and `http/1.1`.
- [ ] Keep certificate private keys outside BN values and logs.
- [ ] Implement bounded client requests, redirects, decoding, and timeouts.
- [ ] Enforce SSRF checks after every DNS result and redirect, with explicit
      CIDR opt-in for local/internal access.
- [ ] Test invalid certificates, ALPN, downgrade refusal, rebinding,
      redirect-to-private, decompression limits, and timeout.

**Exit:** the HTTPS server and client interoperate locally over IPv4 and IPv6;
no cleartext downgrade or default private-network access exists.

## Sprint 6 — Cookies, sessions, and static scraping

WBS: Phase 6.

- [ ] Implement isolated cookie jars with domain/path/expiry and secure flags.
- [ ] Implement bounded in-memory sessions with opaque rotating identifiers,
      expiry, capacity, and eviction.
- [ ] Integrate approved HTML parsing and CSS selectors.
- [ ] Add links, forms, charset, optional robots policy, rate limits, and all
      size/work ceilings.
- [ ] Verify scripts, event handlers, and subresources never execute.

**Exit:** state never crosses unrelated sessions, and scraping is static,
bounded, and testable without the public Internet.

## Sprint 7 — ACL and Apache logs

WBS: Activities 7.1.1–7.1.2.

- [ ] Implement ordered first-match ACL with default deny.
- [ ] Approve one local geolocation database format and data-license policy.
- [ ] Implement fail-closed IPv4/IPv6 local geolocation lookup.
- [ ] Distinguish transport peer, trusted effective origin, HTTP `Origin`, and
      local destination.
- [ ] Emit one Apache Combined-compatible escaped line for every completed or
      rejected exchange.
- [ ] Report failed log writes and test log-injection inputs.

**Exit:** IPv4/IPv6 access decisions and audit records are deterministic and
retain proxy provenance.

## Sprint 8 — QUIC v1 and HTTP/3

WBS: Phase 8.

- [ ] Revalidate and approve the HTTP/3 dependency set immediately before use.
- [ ] Pass a standalone client/server spike against two independent peers.
- [ ] Integrate QUIC over host-mediated UDP and TLS 1.3 with ALPN `h3`.
- [ ] Reuse the same request, route, filter, ACL, session, and logging pipeline.
- [ ] Advertise HTTP/3 through Alt-Svc while preserving HTTPS HTTP/1.1 and
      HTTP/2 fallback.
- [ ] Test cancellation, timeout, ALPN failure, fallback, and independent-peer
      interoperability.

**Exit:** HTTP/3 is claimed only on hosts covered by executable interop
evidence; it is never required for resource access.

## Sprint 9 — Conformance, plugins, and release

WBS: Phase 9 plus inherited gate `G0.1`.

- [ ] Complete grammar, semantic, IR, runtime, identity, capability, and
      protocol matrices.
- [ ] Add concise BN examples for server, client, routes, sessions, scraping,
      ACL, and logs.
- [ ] Update VS Code and Jupyter; keep network unavailable in Jupyter unless a
      real provider and tests are added.
- [ ] Close `G0.1` Windows evidence.
- [ ] Run the full Rust, plugin, packaging, documentation-link, security, and
      protocol interoperability gates.
- [ ] Publish capability matrix, dependency inventory, release notes, and
      evidence.

**Exit:** every mandatory item is checked, optional work remains deferred, and
no target or protocol support is claimed without executable evidence.

## Deferred decisions

- `BNNet`: reconsider only after `HOST.Network` and `BNWeb` prove that a
  diagnostic module adds value without duplicating transport ownership.
- Static files: reconsider after the response API can enforce root confinement,
  MIME, conditional requests, and ranges.
- Parallel BN handlers: reconsider after interpreter thread safety and shared
  state have a separate design and measurements.

## Resume point

Start at **Sprint 0**: produce the dependency/runtime decision record and
freeze the complete typed `HOST.Network` and `BNWeb` APIs. Do not add network,
HTTP, TLS, QUIC, cookie, or HTML dependencies before that gate is approved.
