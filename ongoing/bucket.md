# Basic Next 0.4 Security and Production-Hardened BNWeb Bucket

This bucket supersedes the archived 0.3 delivery bucket for active 0.4 work.
Execution is sequential: only the first sprint containing an unchecked
activity is active. The 0.4 authority boundary now adopts the bounded
`ASYNC`/`AWAIT` amendment; implementation remains gated by the threat-model
and concurrency/provider decisions.

Detailed objectives, dependencies, acceptance criteria, and verification
checks live in [`WBS-0.4.md`](WBS-0.4.md).

All normative resource thresholds are recorded in the versioned registry
[`config/0.4-bnweb-limits.toml`](../config/0.4-bnweb-limits.toml). Runtime code
must consume one typed validated snapshot of that registry; duplicated limit
literals and unbounded fallback defaults are release-gate failures.

## SECTION 1 — Security inventory and authority

### SPRINT 0 — Vulnerability register and threat-model gate

- [X] ACTIVITY 0.1 — Establish the active 0.3 vulnerability register from the code review, archived audit evidence, and the production-hardening findings. Evidence: [`0.4-security-register.md`](0.4-security-register.md), findings BN-SEC-001 through BN-SEC-007.
- [X] ACTIVITY 0.2 — Reconcile the 0.4 prose/EBNF with the `ASYNC`/`AWAIT` design and freeze the normative boundary. Evidence: [`0.4-authority-audit.md`](0.4-authority-audit.md), [`docs/language/0.4/keywords.md`](../docs/language/0.4/keywords.md); Option A accepted on 2026-09-01.
- [X] ACTIVITY 0.3 — Freeze the BNWeb threat model, resource quotas, error mappings, and residual-risk policy. Evidence: [`0.4-threat-model.md`](0.4-threat-model.md); defaults accepted on 2026-09-01.
- [X] ACTIVITY 0.4 — Decide concurrency, task ownership, cancellation, provider, semaphore, response, logging, and shutdown contracts. Evidence: [`0.4-concurrency-decision.md`](0.4-concurrency-decision.md); contract accepted on 2026-09-01.
- [X] ACTIVITY 0.5 — Create deterministic local resolver, clock, entropy-failure, slow-peer, TLS, connection-accounting, and rate-limit-key test seams. Evidence: [`0.4-test-harness.md`](0.4-test-harness.md); local HTTP lifecycle, deadline, cancellation, failure, repeated-stop, and bounded-key tests pass.
- [X] GATE G0 — Active register, 0.4 authority boundary, threat-model defaults, concurrency contract, and deterministic-test seams are recorded with explicit owners and verification evidence. The gate was accepted on 2026-09-01; Activity 0.5 is closed with local lifecycle evidence.

Implementation requirements: tests must not depend on a public Internet host,
wall-clock sleep, externally issued certificate, or real entropy failure.

Exit evidence: every finding has an owner and test; the 0.4 documents no
longer claim to define 0.3; all security and concurrency decisions are explicit.

## SECTION 2 — Network and session security

### SPRINT 1 — SSRF policy and session confidentiality

- [X] ACTIVITY 1.1 — Centralize IPv4/IPv6 sensitive-range classification, including IPv4-mapped IPv6, CGNAT, documentation, and benchmark ranges. Evidence: shared `validate_ssrf_destinations` policy and mapped/special-range regression tests.
- [X] ACTIVITY 1.2 — Apply the same SSRF policy after URL parsing, every DNS result, and every redirect. Evidence: injected resolver path plus mixed-answer and redirect revalidation tests fail closed.
- [X] ACTIVITY 1.3 — Add immutable outbound `EgressPolicy` allowlists for schemes, CIDRs, ports, redirects, and deadlines. Evidence: typed `BNWeb.EgressPolicy`, `Client.RequestWithPolicy`, bounded CSV/CIDR parsing, shared resolver/redirect enforcement, and local runtime/policy tests.
- [X] ACTIVITY 2.1 — Replace sequential `SessionStore` IDs with at least 128 bits from the approved CSPRNG and preserve rotation invalidation. Evidence: random 16-byte IDs, failure injection, uniqueness, rotation invalidation, and capacity tests.
- [X] ACTIVITY 2.2 — Freeze secure cookie defaults and explicit session policy. Evidence: default `Secure`/`HttpOnly`/`SameSite=Lax` metadata, explicit BN `SetWithPolicy`, insecure `SameSite=None` rejection, and 0.4 contract documentation are implemented and tested.
- [X] GATE G1 — SSRF and session tests demonstrate fail-closed classification and non-predictable identifiers. Evidence: mapped/special-range resolver tests, redirect revalidation, CSPRNG failure/rotation tests, secure cookie policy tests, and the public EgressPolicy runtime fixture.

Implementation requirements: the egress policy must recheck every resolved
address and redirect; rate/security decisions must fail closed; session IDs use
at least 128 CSPRNG bits and never a counter, clock, or PID.

Exit evidence: mapped loopback/link-local/private/CGNAT cases are rejected;
global addresses pass; session IDs are random, bounded, and invalidated on
rotation.

## SECTION 3 — HTTP protocol hardening

### SPRINT 2 — Typed options, headers, cookies, and timeout closure

- [X] ACTIVITY 2.3 — Introduce immutable typed `Server.Options` with finite validated configuration for quotas, backlog, target/header/body bounds, timeouts, `trustedProxy`, policies, and TLS-specific behavior. Evidence: versioned registry, immutable Rust snapshot, BN constructor, bounded `TcpSocket::listen(backlog)`, and cleartext/TLS start consumers with validation before bind.
- [X] ACTIVITY 2.4 — Add typed default security response headers with correct HTTPS conditions. Evidence: `X-Content-Type-Options: nosniff` is always applied; HSTS is applied only to TLS responses; conflicting default values are replaced and covered by transport tests.
- [X] ACTIVITY 2.5 — Add handshake, header, body, idle, connection, and shutdown deadlines to cleartext and TLS paths. Evidence: configured header/body/total deadlines, idle TLS handshake, and an HTTP/2 peer that ignores the keep-alive PING are covered by local fixtures. The transport mapping is explicit: body timeout is HTTP 408; header, TLS-handshake, and connection deadlines close the transport with a timeout error; admission overload is HTTP 503; rate limiting is HTTP 429 with `Retry-After`.
- [X] GATE G2 — Slow clients and stalled TLS/HTTP2 peers terminate within configured bounds without weakening existing body/header limits. Accepted on 2026-09-01 after focused timeout fixtures and the full Rust test suite passed.

Implementation requirements: `Start` and `StartTLS` consume exactly the same
validated options; HSTS is never emitted over cleartext; duplicate conflicting
security headers are rejected; invalid options bind no listener.

Exit evidence: HTTP/1.1 and HTTP/2, cleartext and TLS, positive and timeout
tests pass with documented response-header and cookie policies.

## SECTION 4 — Bounded server lifecycle

### SPRINT 3 — Admission, workers, stop, and drain

- [X] ACTIVITY 3.1 — Enforce global connection limits before spawning connection work and define backlog/429/503/close semantics. Evidence: bounded backlog bind, shared cleartext/TLS admission, N+1 rejection, 503 request overload, 429 rate limit, and close-on-connection-cap behavior.
- [X] ACTIVITY 3.2 — Replace per-connection unbounded thread/runtime creation with an approved bounded worker/provider model. Evidence: one fixed worker pool and one shared multi-thread Tokio runtime per server, bounded `sync_channel` from `pending_work`, synchronous handler admission held for the full handler lifetime via RAII, `try_send` rejection, panic isolation, N+1 admission test, and sender/runtime-drop drain/join.
- [X] ACTIVITY 3.3 — Implement listener signaling, connection tracking, drain, join, cancellation, idempotent close, and deadline-aware stop. Evidence: listener handles are transferred out of the mutex and joined within the shared deadline; admitted sockets receive cooperative `shutdown(Both)` cancellation; worker drain observes the requested deadline; slow, failing, repeated-stop, multiple-socket, timeout, and listener-boundary tests pass.
- [X] ACTIVITY 3.4 — Add bounded route-plus-effective-client token-bucket rate limiting with `429` and `Retry-After`. Evidence: bounded integer token bucket, deterministic oldest-key eviction with the evicted key asserted, millisecond refill arithmetic that preserves sub-second remainder, controlled-time refill, trusted-proxy provenance, route/client isolation, and HTTP `429`/`Retry-After: 1` test.
- [X] ACTIVITY 3.5 — Expose read-only server readiness/drain status and counters without creating hidden HTTP routes. Evidence: typed lifecycle statuses, readiness transition, counters, and public stopped/not-ready projection tests.
- [X] ACTIVITY 3.6 — Close native HOST.Net resource and timeout gaps. Evidence: resolver tasks retain JoinHandles and are reaped; TCP connect applies read/write deadlines; TCPListen uses `bind_with_backlog`; TCP accept uses an async deadline instead of 1 ms polling; TCP/UDP handle counts, resolver results, and datagrams use registry limits; mapped IPv4 predicates are classified as IPv4; UDP multicast/broadcast sends are denied. `Ping`, `Neighbor`, and `Reverse` remain explicit provider-deferred capabilities because a portable implementation requires a new permission/timeout/provider contract.
- [X] GATE G3 — Controlled `N+1` load cannot exceed the worker/connection quota, and successful stop leaves no untracked listener or connection worker. Evidence: N+1 admission/pool test, fixed worker/runtime tests, clear overload status tests, and lifecycle join/drain suite.

Implementation requirements: an `N+1` local connection test cannot exceed the
configured admission cap; the pool must create exactly `worker_count` workers,
the queue must never exceed `pending_work`, and each accepted socket must be
released exactly once on success, queue-full rejection, worker panic, timeout,
or stop cancellation; rate-limit keys are bounded and honor trusted-proxy
policy; rejected requests never execute handlers; successful stop accounts for
every admitted connection and joins the listener plus all pool workers before
the deadline. Cleartext and TLS must use the same admission and pool contract.

Required checks for 3.2: unit-test fixed worker count and queue saturation;
exercise both accept paths with a controlled `N+1` load; assert no
`bnweb-connection`/`bnweb-tls-connection` thread is created per socket; inject
worker panic and verify unrelated work continues; stop with queued and active
work and verify deterministic drain, join, and timeout behavior. Acceptance
requires bounded thread count, bounded queue, no leaked admission, and a clear
overload result (`close` until the explicit 503/backlog policy is accepted).
The shared Tokio runtime is dimensioned by `worker_count` and is removed from
server state during stop; only already-admitted work may retain it until drain.

Exit evidence: cleartext and TLS accept paths share quota and lifecycle rules;
overload, worker failure, slow connection, stop, and close are deterministic.

## SECTION 5 — Logging and evidence

### SPRINT 4 — Operational secrecy and conformance

- [X] ACTIVITY 4.1 — Expand BNLog redaction and control-character defenses for secrets, credentials, session material, queries, bodies, and TLS keys. Evidence: case-insensitive denylist, modern token variants, control escaping, query stripping, record bound, and BNWeb field audit tests.
- [X] ACTIVITY 4.2 — Add validated request correlation IDs and bounded read-only server statistics without external telemetry dependencies. Evidence: CSPRNG-backed fresh `X-Request-ID`, saturating request/connection counters, failure/timeout accounting, read-only Rust/BN projections, bounded total/average/maximum request-duration aggregates, BNWeb response/log correlation, concurrent snapshot reads, and cross-format redaction tests.
- [X] ACTIVITY 4.3 — DEFERRED by the 0.4 release decision: atomic TLS certificate reload remains outside the base release and is not claimed as implemented.
- [X] ACTIVITY 4.4 — Record executable security/conformance evidence and residual risks. Evidence: [`0.4-conformance.md`](0.4-conformance.md) maps every accepted hardening artifact to executable local evidence, platform boundaries, and residual-risk decisions; optional TLS reload and persistent Jupyter Session remain explicitly excluded.
- [X] GATE G4 — No production-hardening claim lacks a test, documented default, platform boundary, and evidence record. Accepted on 2026-09-01 with [`0.4-conformance.md`](0.4-conformance.md); optional TLS reload remains excluded.

Implementation requirements: correlation IDs and stats never contain query,
cookie, authorization, body, TLS, descriptor, or peer-secret data; invalid TLS
reload keeps the previous configuration live; no cleartext fallback is valid.

Exit evidence: all supported logging formats redact equivalent secrets and all
required repository quality checks pass.

## SECTION 6 — 0.4 language and concurrent BNWeb

### SPRINT 5 — Async language contract and runtime

- [X] ACTIVITY 5.1 — CLOSED 2026-09-02: restored BNDispatch lifecycle, state-machine, capacity, synchronization, configuration, and task-isolation conformance. Queue workers now disconnect and join within a deadline; cancellation is cooperative before user-code entry; panics become FAILED; pending capacity excludes terminal tickets; barriers break timed-out generations; synchronization ownership and release bounds are checked; poisoned synchronization state no longer aborts the process; self-join from a worker returns `SelfJoin`; thresholds come from typed `DispatchLimits`; ticket IDs are CSPRNG-backed and non-sequential; closed tickets retain failure diagnostics; and task runtimes fork independent host state. The approved recovery design is [`2026-09-02-bndispatch-recovery-design.md`](../docs/superpowers/specs/2026-09-02-bndispatch-recovery-design.md). `Mutex.Unlock`, `Semaphore.Release`, and `Group.Leave` return `VOID OR Error` in the 0.4 provider contract. Evidence: focused BNDispatch tests, BN-DISPATCH-009, 167 runtime tests, full `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and scoped `git diff --check` all pass.
- [X] ACTIVITY 5.2 — Add opt-in concurrent BNWeb handlers over an explicit bounded queue with isolated request/response ownership. Evidence: `ServerOptions.concurrentHandlers` is registry-backed and opt-in; the HTTP transport uses one server-owned semaphore, offloads each handler to a bounded slot, gives it cloned request plus private response ownership, returns 503 without invoking the handler when all slots are occupied, maps failure to 500 and deadline expiry to 408, and counts active blocking handlers during stop/drain. `http::tests::opt_in_concurrent_handler_runs_with_bounded_handler_slot`, `opt_in_concurrent_handler_rejects_when_global_slot_is_occupied`, `opt_in_concurrent_handler_failure_maps_to_internal_error`, `opt_in_concurrent_handler_timeout_maps_to_request_timeout`, `web::tests::stop_does_not_claim_success_with_an_active_concurrent_handler`, and `tests/runtime.rs::bnweb_start_serves_registered_bn_handler_over_http` cover transport, lifecycle, and real BN listener integration. `runtime::tests::web_callback_uses_a_fresh_executor_and_projects_response` covers request-local interpreter ownership.
- [X] DECISION G5-A — Adopt option A: `Server.Start` and `StartTLS` snapshot the module, host capability template, route handler names, and filters; each request creates a fresh `Executor` and marshals bounded `BNWeb.Request`/`BNWeb.Response` values. The live registering executor, its heaps, handles, and output writer are never captured by the HTTP callback. The bridge is implemented in `runtime::execute_web_callback` and `runtime/executor/part4.rs`; remaining acceptance evidence is the end-to-end BN listener fixture and failure/timeout/drain matrix.
- [X] GATE G5 — CLOSED 2026-09-02 after BNDispatch recovery. The native async provider now has bounded worker lifecycle, panic recovery, pending capacity, synchronization misuse diagnostics, typed configuration, retained failures, and independent task runtime evidence. BNWeb concurrent handlers remain governed by Activity 5.2 and its separate response-ownership matrix.

Exit evidence: synchronous 0.3 behavior remains intact; async/concurrent mode
is opt-in, bounded, isolated, observable, and gracefully stoppable.

## SECTION 7 — Debugger and notebook user experience

### SPRINT 6 — Native DAP integration and notebook contract

- [X] ACTIVITY 6.1 — Make `bn dap` deliver events while idle, distinguish next/step-in/step-out at IR instruction boundaries, and expose source-mapped stack snapshots. Evidence: Rust DAP session uses queued asynchronous events, depth-aware `Next`/`In`/`Out`, breakpoints, stack/source spans, and locals snapshots; `dap::tests::execution_session_pauses_then_resumes`, breakpoint/framing/source-line tests, and the native adapter smoke test pass.
- [X] ACTIVITY 6.2 — Replace the VS Code launch-only `runInTerminal` adapter with a bounded local stdio bridge to `bn dap`. Evidence: `plugins/vscode/debugAdapter.js` forwards bounded DAP frames to the child `bn dap` process; `node plugins/vscode/test/debug-adapter.js` verifies queued launch/configuration, native stopped/continued/terminated flow, no `runInTerminal`, and child cleanup; extension checks pass.
- [X] ACTIVITY 6.3 — Document native debugger stepping as source-mapped IR-instruction stepping, never as a REPL or arbitrary-expression evaluator. Evidence: VS Code, usage, language 0.4, and WBS docs state the IR/span stepping model, non-REPL boundary, unsupported targets, and locals snapshot semantics; link and extension checks pass.
- [X] ACTIVITY 6.4 — Freeze Jupyter `Program` mode as the only 0.4 execution model: complete program, fresh process, no filesystem, no state between cells. Evidence: `tests/test_kernel.py` covers complete-program and filesystem-denial behavior; `tests/test_jupyter.py` covers kernel-info, execute/IOPub stream, shutdown, and wire framing with `pyzmq` 27.2.0 in the plugin virtual environment.
- [X] ACTIVITY 6.5 — DEFERRED: persistent Jupyter `Session` mode is explicitly outside 0.4 and requires a new accepted execution/resource contract.
- [X] GATE G6 — Accepted on 2026-09-02: VS Code uses native DAP, Jupyter Program-only mode has Rust/Python/wire evidence, and deferred Session/TLS features are not claimed.

Implementation requirements: the VS Code adapter forwards DAP rather than
reimplementing debug semantics; `runInTerminal` is never a debug path;
`stopped` must arrive without a second client request; source line mapping is
derived from IR spans. Jupyter Program mode remains the default, requires a
complete `FUNCTION Start()` program, runs in a fresh process, and shares no
state between cells. A Session mode requires explicit transactional and
resource bounds before code begins.

Exit evidence: local DAP protocol and VS Code extension tests cover launch,
breakpoints, pause, all step commands, source/locals snapshots, child cleanup,
and malformed framing. Jupyter tests cover Program-mode isolation, filesystem
denial, input, heartbeat, interrupt, and shutdown; Session tests are required
only if the optional mode is accepted.

## SECTION 8 — Release integration

### SPRINT 7 — 0.4 release gate

- [ ] ACTIVITY 6.6 — REOPENED 2026-09-02: the release gate cannot close while BNDispatch Activity 5.1 is open. Release documentation records the advisory; final language/module docs, examples, plugins, conformance, and full quality evidence must be rerun after recovery.
- [ ] GATE G7 — REOPENED 2026-09-02: the 0.4 release remains published source, but cannot claim final conformance until G5 closes with BNDispatch recovery evidence. Optional TLS reload and Jupyter Session remain deferred and unclaimed.

Exit evidence: the accepted normative contract, implementation, security
register, conformance evidence, examples, and plugins are mutually consistent.
