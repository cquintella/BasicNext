# Basic Next 0.4 Security and Production-Hardened BNWeb

## Phase 0 — Authority, inventory, and threat-model gate

### Vulnerability inventory

#### Activity 0.1 — Establish the active 0.3 vulnerability register

- **Status:** DONE.
- **Objective:** Replace the absence of a consolidated active vulnerability list with an evidence-based register derived from the 0.3 implementation, archived audits, and the production-hardening review.
- **Deliverables:** [`ongoing/0.4-security-register.md`](0.4-security-register.md); findings for IPv4-mapped SSRF, incomplete special-range filtering, predictable session IDs, unbounded per-connection thread creation, incomplete accept/stop lifecycle, missing default security headers/cookie policy, and insufficiently broad log redaction.
- **Dependencies:** Existing 0.3 source, `done/project/audit-sprint-*.md`, `done/0.3-decisions.md`, `docs/language/0.3/bnweb.md`, and this WBS.
- **Acceptance criteria:** Every finding has an identifier, severity, affected files/functions, root cause, exploit or failure scenario, mitigation, regression test, documentation impact, residual risk, and WBS owner. Historical audit files are explicitly labeled as evidence, not as the active register.
- **Verification:** `rg --files | rg -i 'vuln|security|threat|risk|audit|cve'`; review all matches; confirm the register contains BN-SEC-001 through BN-SEC-007 and no finding is left without an owner and acceptance check. **Evidence:** `ongoing/0.4-security-register.md`.

#### Activity 0.2 — Freeze the 0.4 authority boundary

- **Status:** DONE — Option A accepted on 2026-09-01.
- **Objective:** Resolve the fact that `docs/language/0.4/0.4.md` still describes 0.3, while `docs/language/0.4/0.4.ebnf` does not contain the `ASYNC`/`AWAIT` syntax defined by the separate async design.
- **Deliverables:** [`ongoing/0.4-authority-audit.md`](0.4-authority-audit.md); corrected 0.4 status, authority order, scope statement, grammar references, and an explicit relationship to `docs/superpowers/specs/2026-09-01-async-await-0.4-design.md`; no security behavior is accepted merely because it appears in a forward plan.
- **Dependencies:** Activity 0.1; `docs/language/0.4/0.4.md`; `docs/language/0.4/0.4.ebnf`; async design; `done/0.4-forward-plan.md`.
- **Acceptance criteria:** The normative 0.4 text identifies itself as 0.4, the EBNF and prose agree on every new keyword and production, and unresolved API/provider/security choices are explicit decision gates.
- **Verification:** Compared the 0.4 prose, EBNF, keyword registry, and async design; confirmed `ASYNC`/`AWAIT` are defined consistently and absent from the 0.3 contract. `git diff --check` and targeted `rg` checks passed. **Evidence:** [`docs/language/0.4/0.4.md`](../docs/language/0.4/0.4.md), [`docs/language/0.4/0.4.ebnf`](../docs/language/0.4/0.4.ebnf), [`docs/language/0.4/keywords.md`](../docs/language/0.4/keywords.md), [`ongoing/0.4-authority-audit.md`](0.4-authority-audit.md).

### Threat-model and dependency gate

#### Activity 0.3 — Freeze the production BNWeb threat model and quotas

- **Status:** DONE — defaults accepted on 2026-09-01.
- **Objective:** Define what an untrusted or locally supplied `.bn` program can cause through `HOST.Net`, `BNWeb`, `HOST.FileSystem`, TLS, sessions, logging, and process resources.
- **Deliverables:** [`ongoing/0.4-threat-model.md`](0.4-threat-model.md), covering SSRF, DNS/redirect rebinding, connection/file-descriptor/thread exhaustion, slow clients, oversized headers/bodies, TLS handshake abuse, session guessing, log injection/secrets, shutdown races, and clear capability-unavailable behavior; quota table for connections, backlog, pending requests, headers, body, idle time, handshake, and shutdown.
- **Dependencies:** Activity 0.1; `done/0.3-decisions.md`; OWASP ASVS/session guidance and Mozilla TLS guidance as reviewed references.
- **Acceptance criteria:** Every resource and trust boundary has an owner, finite default, configurable range, rejection/error mapping, and shutdown behavior. No new dependency or network service is introduced without an approved decision.
- **Verification:** Reviewed the threat model against public BNWeb operations and both cleartext/TLS accept paths; confirmed all thresholds are represented in [`config/0.4-bnweb-limits.toml`](../config/0.4-bnweb-limits.toml); Carlos accepted the quota table, error mapping, residual-risk policy, and lifecycle invariants on 2026-09-01. **Evidence:** [`ongoing/0.4-threat-model.md`](0.4-threat-model.md).

#### Activity 0.4 — Decide the 0.4 concurrency/provider contract

- **Status:** DONE — contract accepted on 2026-09-01.
- **Objective:** Decide whether asynchronous BN execution and concurrent BNWeb handlers are permitted, and freeze ownership, cancellation, failure, response completion, output ordering, and provider dependencies before implementation.
- **Deliverables:** [`ongoing/0.4-concurrency-decision.md`](0.4-concurrency-decision.md); accepted decisions for `BNDispatch`, fixed workers, queue bounds, task isolation, running-task cancellation, concurrent handler response ownership, and the relationship between `BNDispatch` and `ASYNC`/`AWAIT`.
- **Dependencies:** Activity 0.2; `docs/superpowers/specs/2026-09-01-async-await-0.4-design.md`; `done/0.4-forward-plan.md`.
- **Acceptance criteria:** No implementation starts while grammar, parameter ownership, cancellation, graceful shutdown, or provider policy remains ambiguous.
- **Verification:** Reviewed the decision record against the exact grammar, accepted quota table, and runtime tests required by later phases; Carlos accepted the contract on 2026-09-01. **Evidence:** [`ongoing/0.4-concurrency-decision.md`](0.4-concurrency-decision.md).

#### Activity 0.5 — Build deterministic security test harnesses

- **Status:** DONE.
- **Objective:** Make security and resource-limit tests deterministic without public Internet access, wall-clock sleeps, or nondeterministic operating-system timing.
- **Deliverables:** [`ongoing/0.4-test-harness.md`](0.4-test-harness.md); test-only resolver, clock/deadline, CSPRNG-failure, socket/slow-peer, TLS-handshake, bounded rate-limit-key, and connection-accounting seams; fixtures remain local and certificate material remains test-only.
- **Dependencies:** Activities 0.1 and 0.3; `src/web.rs`; `src/http.rs`; `src/web_state.rs`; `src/net.rs`.
- **Acceptance criteria:** SSRF, redirect, session-randomness failure, rate-limit, overload, handshake-timeout, idle-timeout, drain, and TLS tests can control their inputs and complete without a public service.
- **Verification:** `src/test_support.rs` now provides and tests bounded `FakeResolver`, `ManualClock`, `TestDeadline`, delayed `ScriptedPeer`, `HandshakeResult`, `BoundedKeyTable`, and `ConnectionAccounting`, including partial-header/body timing, deterministic deadline expiration, rate-limit key saturation, and release/join accounting; the fake resolver is exercised by an SSRF policy test, `src/http.rs` consumes registered TLS-handshake, HTTP/1 header, HTTP/2 idle, and body deadlines, and local HTTP lifecycle tests cover cancellation, slow/failing workers, listener/worker deadlines, repeated stop, and multiple sockets. Focused verification: `cargo fmt --check`, `cargo test test_support::tests --lib` (6 passed), `cargo test web::tests --lib`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check` passed.

#### Gate G0 — Release authority and security decision gate

- **Status:** DONE — accepted on 2026-09-01.
- **Objective:** Confirm that the active findings, 0.4 language boundary, threat-model defaults, concurrency/provider contract, and test-harness requirements are explicit before feature implementation proceeds.
- **Deliverables:** [`ongoing/0.4-security-register.md`](0.4-security-register.md), [`ongoing/0.4-authority-audit.md`](0.4-authority-audit.md), [`ongoing/0.4-threat-model.md`](0.4-threat-model.md), [`ongoing/0.4-concurrency-decision.md`](0.4-concurrency-decision.md), and [`ongoing/0.4-test-harness.md`](0.4-test-harness.md).
- **Dependencies:** Activities 0.1–0.5; Carlos's acceptance of the 0.4 authority, quota defaults, and concurrency contract.
- **Acceptance criteria:** No unresolved authority/security/provider choice is silently assumed; every high/critical finding has an owner and regression target; `ASYNC`/`AWAIT` is normative only in 0.4; limits are recorded in the versioned registry; implementation activities can reference a reproducible local test seam.
- **Verification:** Reviewed the five decision/evidence records and confirmed the accepted decisions dated 2026-09-01. Focused harness, HTTP, lint, format, and diff checks pass; full repository verification also passed in the preceding continuation.

## Phase 1 — SSRF and network-policy hardening

### Canonical destination classification

#### Activity 1.1 — Centralize IPv4 and IPv6 sensitive-range classification

- **Status:** DONE.
- **Objective:** Make one policy function classify all client destinations, including IPv4-mapped IPv6 addresses and special IPv4 ranges.
- **Deliverables:** A helper used by URL-literal validation, post-DNS validation, and redirect validation. IPv4 policy covers RFC1918, loopback, link-local, unspecified, multicast, CGNAT `100.64.0.0/10`, documentation ranges, and benchmark `198.18.0.0/15`; IPv6 policy covers loopback, link-local, multicast, unspecified, ULA `fc00::/7`, and mapped IPv4 values by applying the IPv4 policy.
- **Dependencies:** Activity 0.3; `src/web.rs`; `src/http.rs`; `src/runtime/executor/part4.rs`.
- **Acceptance criteria:** `allow_private == false` rejects `::ffff:127.0.0.1`, `::ffff:10.0.0.1`, `::ffff:169.254.169.254`, and CGNAT, while a documented global IPv6 and public IPv4 address pass. Explicit opt-in preserves the existing private-address contract.
- **Verification:** `src/web/tests.rs` covers mapped loopback/private/link-local values, CGNAT, documentation and benchmark ranges, public IPv4/IPv6 values, empty resolution, and the explicit private-address opt-in. URL-literal and post-resolution paths call the shared `validate_ssrf_destinations` helper. `cargo test web::tests --lib` and the full repository suite pass.

#### Activity 1.2 — Preserve SSRF policy through resolution and redirects

- **Status:** DONE.
- **Objective:** Ensure every returned address is checked after each resolver result and every redirect, without trusting the original hostname or an earlier resolution.
- **Deliverables:** One post-resolution policy call per request/redirect hop; bounded redirect count; tests using resolver doubles for mixed public/private results and mapped addresses.
- **Dependencies:** Activity 1.1; `src/http.rs`; `src/web.rs`.
- **Acceptance criteria:** A request fails closed if any resolved address violates the default policy; no redirect bypasses the policy; no public-Internet dependency is required.
- **Verification:** `AddressResolver` injection preserves the system resolver as the production default while allowing bounded local answers; `resolve_validated_addresses` applies `validate_ssrf_destinations` after every resolution. `client_resolver_rechecks_mixed_dns_answers_fail_closed` covers mixed public/mapped-private DNS, public success, and CGNAT rejection; `redirect_destination_is_revalidated_with_the_same_resolver_policy` covers redirect to link-local metadata space. `cargo test http::tests --lib` (13 passed), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` passed.

#### Activity 1.3 — Add an explicit outbound `EgressPolicy`

- **Status:** DONE.
- **Objective:** Offer a stronger, application-selected boundary than a global denylist for BNWeb client traffic.
- **Deliverables:** Immutable typed Rust policy in `src/web.rs`, integrated with the bounded client resolver and redirect loop; public `BNWeb.EgressPolicy` and `Client.RequestWithPolicy` expose the same validated policy to BN programs. It permits only declared schemes, destination CIDRs, ports, redirect count, and total deadline; hostname input is resolved and every resulting address is rechecked against the CIDR policy. The default remains deny-private plus the existing bounded client behavior.
- **Dependencies:** Activities 0.3, 0.5, 1.1, and 1.2; `src/web.rs`; `src/http.rs`; BNWeb normative contract.
- **Acceptance criteria:** An allowed hostname cannot escape through DNS rebinding, a redirect to an undeclared port/CIDR fails closed, an empty allowlist permits no outbound traffic, and policy evaluation never performs a second hidden resolver or socket operation.
- **Verification:** `web::tests::egress_policy_is_immutable_and_fail_closed` covers allowed public destination, denied port, denied CGNAT destination, unsupported scheme, empty allowlists, and list capacity; `http::tests::explicit_egress_policy_is_applied_after_resolution` proves port/CIDR policy is applied to resolver output; `bnweb_egress_policy_is_constructed_and_applied_before_transport` validates public BN construction and fail-closed dispatch. Existing resolver-double tests cover mapped/private/CGNAT destinations, mixed DNS responses, redirects, and unchanged default policy. Registry parity is enforced by `egress_list_max`, `redirects_max`, and the typed snapshot.

## Phase 2 — Session and HTTP protocol hardening

### Session confidentiality

#### Activity 2.1 — Replace sequential session IDs with CSPRNG IDs

- **Status:** DONE.
- **Objective:** Remove session identifier predictability and retain rotation invalidation semantics.
- **Deliverables:** `SessionStore::create` uses `ring::rand::SystemRandom` or the approved cryptographic provider to generate at least 16 random bytes, encoded as a bounded hex or base64url token; `rotate` removes the old ID before creating a new random ID; no counter, time, or PID contributes to the ID.
- **Dependencies:** Activity 0.3; existing `ring` dependency; `src/web_state.rs`; `src/runtime/executor/part17.rs`.
- **Acceptance criteria:** IDs meet the minimum entropy/length contract, consecutive IDs are not adjacent, old IDs fail after rotation, and random-provider failure returns a typed deterministic error without inserting a session.
- **Verification:** `src/web_state.rs` uses the approved `ring::rand::SystemRandom` provider to generate 16 random bytes; tests cover bounded length, non-adjacent consecutive IDs, rotation invalidation, capacity eviction, and deterministic entropy-provider failure without insertion. `cargo test` passes.

#### Activity 2.2 — Freeze secure cookie defaults and session policy

- **Status:** DONE.
- **Objective:** Make cookie security properties explicit and safe for production sessions.
- **Deliverables:** Typed Rust `CookieOptions`/`SameSite` metadata with defaults `Secure=true`, `HttpOnly=true`, and `SameSite=Lax`, applied by the existing `CookieJar` API; BN `SetWithPolicy` exposes explicit flags and rejects `SameSite=None` without `Secure`. No cookie or session identifier is logged by default.
- **Dependencies:** Activity 2.1; `docs/language/0.3/bnweb.md`; `src/web_state.rs`; `src/log.rs`.
- **Acceptance criteria:** Production session examples use secure defaults, policy is observable through typed API behavior, and compatibility exceptions are explicit rather than implicit.
- **Verification:** `web_state::tests::cookie_defaults_are_secure_http_only_and_lax` proves the default metadata; `bnweb_cookie_jar_exposes_explicit_secure_policy` covers explicit `Strict` policy and rejects insecure `SameSite=None`; existing expiry/domain/path tests remain green. The 0.4 contract documents defaults, allowed policies, and the no-cookie/session-log rule. `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check` are required before gate closure.

### HTTP response and transport timeouts

#### Activity 2.3 — Introduce immutable typed `Server.Options`

- **Status:** DONE.
- **Objective:** Consolidate server security configuration so listeners do not acquire divergent hidden defaults.
- **Deliverables:** [`config/0.4-bnweb-limits.toml`](../config/0.4-bnweb-limits.toml) as the single threshold registry; a typed immutable Rust `ServerOptions` snapshot with validated connection/backlog/pending/worker quotas, header/target/body bounds, all transport/shutdown timeout fields, and explicit `trustedProxy` provenance policy. `ServerState` captures the snapshot before entering `Accepting`, admission quotas consume it, and the HTTP path enforces configured header-field/header-byte/target limits. `BNWeb.ServerOptions.New(...)`, `StartWithOptions`, and `StartTLSWithOptions` expose the validated numeric snapshot; `TcpSocket::listen(backlog)` applies the same bounded backlog to both listeners, while response/cookie/rate-limit/egress policy fields have their own typed paths.
- **Dependencies:** Activities 0.3 and 0.5; `modules/bn/BNWeb.bn`; `src/runtime/executor/part4.rs`; `src/http.rs`; `src/web.rs`; `src/web_state.rs`.
- **Acceptance criteria:** `Start` and `StartTLS` consume the same validated options; no runtime path substitutes an unbounded timeout or connection limit; `Server.Options` cannot be mutated after a server starts; transport-specific TLS behavior is selected only by `StartTLS` and no TLS-only field is silently ignored by cleartext; no production module duplicates a registry threshold literal.
- **Verification:** `web::tests::server_options_validate_before_accepting_and_bound_quotas` covers a custom bounded snapshot, quota enforcement, and rejection of zero capacity; `net::tests::tcp_listener_bind_with_backlog_uses_bounded_socket_queue` covers the backlog bind path; `bnweb_server_options_are_validated_and_consumed_by_start` covers the BN constructor and start path; `config::tests` covers registry defaults and default/max validation; module graph/parser/semantic suites accept the updated signature; `request_does_not_trust_forwarded_for_by_default` and `proxy_provenance_requires_explicit_trust_and_valid_ip` cover the `trustedProxy` security default. Complete snapshot and bind-before-start validation are covered; timeout parity remains owned by Activity 2.5.

#### Activity 2.4 — Add default security response headers

- **Status:** DONE.
- **Objective:** Provide safe, typed response security headers without allowing duplicate or conflicting defaults to be silently overridden.
- **Deliverables:** Default policy for `Strict-Transport-Security` on HTTPS responses and `X-Content-Type-Options: nosniff`; explicit opt-in/configuration for other headers such as CSP and framing policy; documentation of proxy/TLS termination behavior.
- **Dependencies:** Activities 0.3 and 2.3; `src/http.rs`; `src/web.rs`; BNWeb contract.
- **Acceptance criteria:** Headers are emitted only under their defined transport/policy conditions, remain bounded and valid, cannot be duplicated into conflicting values, and do not claim HSTS on cleartext responses. A strict profile contains `X-Content-Type-Options: nosniff`; CSP and frame policy remain explicit application/policy choices.
- **Verification:** `http::tests::default_security_headers_are_transport_aware` verifies `X-Content-Type-Options: nosniff`, HTTPS-only HSTS, and replacement of a conflicting value; local HTTP/1.1 and HTTP/2 route tests exercise the response path. The implementation emits bounded defaults and does not introduce CSP or frame policy without an explicit application policy. `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check` pass.

#### Activity 2.5 — Close the timeout cycle from accept through stop

- **Status:** DONE.
- **Objective:** Bound handshake, header, body, idle, connection, and shutdown waits for cleartext and TLS servers.
- **Deliverables:** Explicit timeout configuration with conservative defaults and validated ranges; TLS handshake timeout; header/read/idle timeout; connection deadline; stop deadline; deterministic timeout status/error mapping. The typed server snapshot now supplies TLS handshake, header, idle, body, and total connection deadlines to both cleartext and TLS HTTP serving paths; stop deadline remains in the lifecycle drain path. The mapping contract is: body timeout → HTTP 408; header/TLS-handshake/connection deadline → bounded transport close plus `TimedOut`; admission overload → HTTP 503; rate limit → HTTP 429 with `Retry-After`.
- **Dependencies:** Activities 0.3, 0.5, and 2.3; `src/http.rs`; `src/runtime/executor/part4.rs`; `src/web_state.rs`.
- **Acceptance criteria:** A slow client cannot hold a connection indefinitely, a stalled TLS handshake expires, body/header bounds remain enforced, and all waits terminate within the configured deadline.
- **Verification:** `http::tests::configured_header_timeout_closes_a_partial_request`, `configured_body_timeout_returns_request_timeout`, `configured_connection_deadline_closes_a_stalled_request`, and `configured_tls_handshake_timeout_closes_an_idle_peer` exercise the snapshot-backed cleartext/TLS timeout paths with local peers. `http::tests::configured_http2_idle_keep_alive_closes_a_peer_that_ignores_ping` sends a valid HTTP/2 preface/SETTINGS sequence, intentionally withholds the PING acknowledgement, and proves the configured keep-alive cycle closes the peer. Existing HTTP/1.1, HTTP/2, body-limit, overload, and stop/drain tests remain green. Timeout/status mapping is documented and asserted where a protocol response exists; transport-only boundaries return a typed timeout/close error. Focused and full checks passed: `cargo test --all-targets -- --test-threads=1`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check`.

#### Gate G2 — Timeout closure

- **Status:** DONE — accepted on 2026-09-01.
- **Objective:** Confirm that cleartext/TLS slow peers and HTTP/2 keep-alive peers cannot hold resources beyond the configured deadlines.
- **Deliverables:** Registry-backed timeout snapshot, protocol-specific timeout mapping, and local cleartext/TLS/HTTP2 timeout fixtures.
- **Dependencies:** Activities 0.3, 0.5, 2.3, and 2.5.
- **Acceptance criteria:** Header/body/connection/TLS/HTTP2 idle waits are bounded; body timeout maps to 408; transport-only timeouts close and report `TimedOut`; existing body/header limits remain active.
- **Verification:** Activity 2.5 fixtures and `cargo test --all-targets -- --test-threads=1` passed; no public network or wall-clock sleep is required by the fixture.

## Phase 3 — Bounded server lifecycle and overload control

### Admission control

#### Activity 3.1 — Bound connections, backlog, and accept rejection

- **Status:** DONE.
- **Objective:** Prevent unbounded thread, task, descriptor, and memory growth on every accept path.
- **Deliverables:** Shared connection admission counter with validated `max_connections` and backlog snapshot before connection work; both cleartext and TLS accept loops use it, and over-cap sockets are closed. Backlog is applied through the bounded `TcpSocket::listen` provider. Request-queue overload maps to 503, rate limiting maps to 429, and connection-cap rejection closes the accepted socket without creating work.
- **Dependencies:** Activities 0.3, 0.4, 0.5, and 2.3; `src/runtime/executor/part4.rs`; `src/net.rs`; `src/http.rs`.
- **Acceptance criteria:** Both cleartext and TLS listeners enforce the same quota before spawning work; `N+1` controlled connections do not increase active workers beyond `N`; accept errors and overload are distinguishable.
- **Verification:** `server_admission_bounds_connections_before_worker_spawn` and `server_admission_rejects_n_plus_one_with_bounded_pool` cover bounded active connections and N+1 rejection; `net::tests::tcp_listener_bind_with_backlog_uses_bounded_socket_queue` covers the configured backlog; `http::tests::overloaded_http_request_returns_503_without_running_handler` covers request overload and handler isolation; `http::tests::rate_limited_http_request_returns_429_and_retry_after` covers the distinct rate-limit response. Cleartext and TLS accept loops share the same admission implementation and error/close branch; lifecycle tests cover cleanup and worker failure recovery.

#### Activity 3.2 — Replace per-connection runtime/thread creation with bounded execution

- **Status:** DONE.
- **Objective:** Reuse an approved shared Tokio runtime or fixed worker model rather than creating an unbounded thread/runtime per connection.
- **Deliverables:** Fixed worker pool created once per server from the validated `worker_count`; bounded `sync_channel` sized by `pending_work`; one shared multi-thread Tokio runtime is created per server with the same worker bound; cleartext and TLS accept loops submit `ConnectionWork` with `try_send` and release the admitted socket on queue saturation or pool unavailability; synchronous handler admission remains occupied for the full callback lifetime and releases through RAII; worker panics are isolated; stop drops the sender and runtime reference so workers drain queued work and are joined before successful close. No connection thread or runtime is created from the accept path.
- **Dependencies:** Activities 0.4 and 3.1; `BNDispatch` contract; `src/http.rs`.
- **Acceptance criteria:** Worker and pending-task counts have finite configured maxima; task failure cannot poison unrelated connections; no unsafe shared interpreter state is introduced.
- **Verification:** `web::tests::server_worker_pool_has_fixed_workers_and_bounded_queue` proves the configured worker count, queue saturation, and post-stop worker join; `web::tests::server_worker_capacity_is_checked_before_spawn` remains a compatibility regression test; `web::tests::server_admission_rejects_n_plus_one_with_bounded_pool` proves N+1 admission rejection without worker growth; `http::tests::overloaded_http_request_returns_503_without_running_handler` proves request overload while a synchronous handler owns the bounded slot; `bnweb_server_lifecycle_uses_native_state` passes with the shared runtime path. Listener-specific N+1 and overload status evidence is owned by Activity 3.1/G3, not this worker implementation.

### Graceful lifecycle

#### Activity 3.3 — Implement real stop, drain, join, and close deadlines

- **Status:** DONE.
- **Objective:** Make `Stop(timeout)` stop accepting, signal active connections, drain admitted work, join listener/connection tasks, and return a clear timeout/error when the deadline expires.
- **Deliverables:** Listener handle and fixed-pool worker handles owned by server state; idempotent stop/close state machine; pending work cancellation policy; deadline-aware drain/join; no “clear pending and return” behavior that leaves live workers untracked. Current slice stores the listener and pool `JoinHandle`s, observes and joins the listener only while its handle is finished before the shared monotonic deadline (restoring it on listener timeout), retains bounded clones of admitted sockets, requests cooperative `shutdown(Both)` cancellation during drain, drops the pool sender, drains queued work, joins workers, and uses the remaining deadline while polling worker drain. `Stop` transitions to draining and returns a bounded timeout while admitted connections remain; `active_connections()` exposes read-only accounting for tests.
- **Dependencies:** Activities 2.5, 3.1, and 3.2; `src/web_state.rs`; `src/runtime/executor/part4.rs`.
- **Acceptance criteria:** After successful stop/close, no listener accepts new sockets, all admitted work is accounted for, repeated stop/close is deterministic, and a stuck connection yields a bounded timeout result.
- **Verification:** `server_stop_does_not_claim_success_before_connections_drain` covers active-connection rejection, preserved accounting, failed `close`, release, and subsequent stop/close success; `server_reaps_finished_connection_workers` covers deterministic worker-handle cleanup; `server_stop_cancels_an_admitted_socket_without_sleep` proves local TCP peer cancellation via EOF; `server_stop_cancels_multiple_admitted_sockets` covers two active sockets; `server_stop_is_idempotent_after_drain` covers repeated stop/close; `server_stop_drains_a_slow_http_worker` covers a real HTTP worker with partial headers; `failed_connection_worker_is_reaped_without_leaking_admission` covers panic-safe worker cleanup; `drain_server_times_out_without_holding_the_state_lock` covers bounded timeout with a blocked worker; `drain_server_bounds_listener_join_at_the_minimum_timeout` covers listener timeout at 1 ms and subsequent recovery. Focused checks `cargo test web::tests --lib`, `cargo test runtime::executor::part4::tests --lib`, `cargo test --test runtime bnweb_server_lifecycle_uses_native_state`, `cargo test --test runtime bnweb_server_errors_stay_in_error_channel`, `cargo test --lib -- --test-threads=1` (134 tests passed), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check` passed.

#### Activity 3.4 — Add bounded typed route rate limiting

- **Status:** DONE.
- **Objective:** Limit abusive request rates before handler execution without allowing attacker-controlled keys to exhaust memory.
- **Deliverables:** Bounded integer token bucket configured through `ServerOptions`, keyed by route plus `effective_client_address`; fixed burst/refill rates and maximum key count are recorded in the TOML registry. Requests rejected before handler execution return `429 Too Many Requests` with bounded `Retry-After: 1`; forwarded client identity is used only when the immutable `trustedProxy` option is true, otherwise the TCP peer remains authoritative. Full tables evict the oldest key deterministically, with key-name tie breaking, and refill uses injected monotonic timestamps in tests.
- **Dependencies:** Activities 0.3, 0.5, 2.3, and 3.1; `src/web.rs`; `src/http.rs`; `src/web_state.rs`.
- **Acceptance criteria:** Untrusted forwarded headers cannot select the key unless the proxy is configured as trusted; a full key table has deterministic admission/eviction behavior; rejected requests do not invoke filters or handlers; rate limits do not replace the connection cap or overload `503` policy.
- **Verification:** `web::tests::rate_limit_rejects_before_handler_and_bounds_keys` covers initial burst rejection, handler non-execution, and bounded-key behavior; `rate_limit_refills_with_controlled_time_and_evicts_oldest_key` covers millisecond refill, route/client isolation, and deterministic eviction by asserting the evicted and retained keys; `rate_limit_refills_fractional_seconds_without_losing_remainder` covers sub-second refill at 10/s and remainder retention; proxy provenance tests cover trusted/untrusted forwarded identity; `http::tests::rate_limited_http_request_returns_429_and_retry_after` verifies the HTTP status and header. The runtime `ServerOptions` fixture covers rate-limit configuration parsing.

#### Activity 3.5 — Expose readiness and drain state without magic routes

- **Status:** DONE.
- **Objective:** Let applications and hosts inspect lifecycle state while retaining ownership of public HTTP routing.
- **Deliverables:** Read-only BN `Server.Status()`, `IsReady()`, `ActiveConnections()`, and `PendingRequests()` APIs backed by typed `ServerStatus` values `Starting`, `Accepting`, `Draining`, `Stopped`, and `Failed`; no hidden HTTP route is created. Application health-route guidance remains pending.
- **Dependencies:** Activities 3.1–3.3; `src/web_state.rs`; `src/runtime/executor/part4.rs`; `modules/bn/BNWeb.bn`.
- **Acceptance criteria:** Status never exposes descriptors, peer addresses, secrets, or mutable handles; transition ordering is deterministic; readiness becomes false before drain; a stopped server never reports accepting.
- **Verification:** `web::tests::server_status_tracks_readiness_and_drain` covers state/readiness and failed-state transitions; `bnweb_server_status_and_counters_are_read_only` verifies the BN surface returns status and zero counters without a hidden route; `bnweb_server_start_binds_host_net_endpoint` verifies public `Stopped`/not-ready projection after stop; module graph/parser/semantic suites accept the methods. Failure transitions are represented by the native state machine and no public API exposes mutable handles or transport secrets.

## Phase 4 — Logging, secrets, and operational evidence

### Observability safety

#### Activity 4.1 — Expand BNLog secret redaction and log-injection defenses

- **Status:** DONE.
- **Objective:** Prevent credentials, session material, sensitive headers, query data, body data, TLS keys, and control characters from entering production logs through explicit or BNWeb-generated fields.
- **Deliverables:** Expanded case-insensitive denylist covering authorization variants, cookies, session IDs, API keys, passwords, tokens, private keys, proxy credentials, query/body fields, and common secret names; Apache request/referrer query stripping; bounded escaping and record size behavior.
- **Dependencies:** Activity 0.3; `src/log.rs`; `src/http.rs`; `docs/language/0.3/bnlog.md`.
- **Acceptance criteria:** BNWeb defaults remain body/query/cookie/session/TLS-key free; explicit fields cannot bypass mandatory redaction; Apache/text/JSON formats all escape controls and redact equivalent data.
- **Verification:** `log::tests::formats_escape_controls_and_exclude_sensitive_fields` covers authorization, API-key, client-secret, refresh-token, bearer, and JWT variants across JSON/text field formatting; `apache_format_escapes_controls` verifies control escaping and query removal; oversized-record rejection remains covered. The BNWeb-generated fields are limited to method, path without query, and status in `log_web_dispatch`. `cargo test --lib log::tests`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` pass.

#### Activity 4.2 — Add request correlation and bounded server statistics

- **Status:** DONE.
- **Objective:** Make production diagnosis possible without exposing request content or creating an external monitoring dependency.
- **Deliverables:** A request ID generated from approved random bytes or accepted only after syntax/length validation; propagation to BNWeb responses and redacted BNLog fields under explicit policy; a read-only Rust `ServerStats` snapshot and BN projection (`AcceptedRequests`, `ActiveRequests`, `RejectedRequests`, `TimedOutRequests`, `CompletedRequests`, `FailedRequests`, `RateLimitedRequests`, and bounded total/average/maximum request-duration aggregates).
- **Dependencies:** Activities 0.5, 2.1, 2.3, 3.1, 3.4, and 4.1; `src/http.rs`; `src/web.rs`; `src/log.rs`; `src/web_state.rs`.
- **Acceptance criteria:** Request IDs and statistics contain no URL query, cookies, authorization, body, TLS material, or peer descriptor; invalid supplied IDs are replaced or rejected by the accepted policy; counter overflow has specified saturating/error behavior; stats collection cannot block request completion.
- **Verification:** `web_state::tests::request_ids_are_bounded_and_entropy_backed` covers size, uniqueness, and entropy failure; `web::tests::server_stats_are_read_only_and_saturating` covers accepted/completed/active accounting, connection timeout/failure counters, and saturating failure behavior; `web::tests::server_stats_snapshot_supports_concurrent_reads` covers concurrent read-only snapshots; `bnweb_server_status_and_counters_are_read_only` covers the BN projection, including duration metrics; `bnweb_dispatch_correlates_response_id_with_redacted_log_record` proves the same fresh ID is present in the response and BNLog record without query/body leakage; `dispatch_with_key_at` records bounded duration aggregates. `cargo test --test runtime bnweb_dispatch_correlates_response_id_with_redacted_log_record` passed with the local TCP fixture outside the restricted network sandbox; required formatting, lint, and diff checks pass.

#### Activity 4.3 — Support optional TLS certificate reload

- **Status:** DEFERRED — explicitly excluded from the base 0.4 release.
- **Objective:** Allow rotation of externally supplied TLS certificate/key material for new connections without a process restart.
- **Deliverables:** Typed reload operation that validates a complete replacement configuration before atomically publishing it to new TLS handshakes; existing connections retain their original configuration; private material remains provider-owned and redacted.
- **Dependencies:** Activities 2.3, 2.5, 3.1–3.3, and 4.1; `src/tls.rs`; `src/http.rs`; `src/runtime/executor/part4.rs`.
- **Acceptance criteria:** Invalid replacement leaves the prior configuration live; no cleartext fallback occurs; reload races cannot expose partial certificate/key pairs; stop/close coordinates with reload deterministically.
- **Verification:** No implementation or release claim exists for TLS reload; a future release must add the listed certificate fixtures before accepting this activity.

#### Activity 4.4 — Add production security/conformance evidence

- **Status:** DONE.
- **Objective:** Make every hardening claim reproducible and separate local evidence from unsupported platform claims.
- **Deliverables:** [`ongoing/0.4-conformance.md`](0.4-conformance.md), dependency/security evidence, local overload/timeout/SSRF/session/TLS tests, and explicit residual-risk table.
- **Dependencies:** Activities 1.1–4.2; Activity 4.3 only if accepted; 0.4 authority corrections.
- **Acceptance criteria:** No “production-hardened” claim remains without an executable check, documented default, platform boundary, and a test-harness fixture. Optional TLS reload is not claimed if Activity 4.3 is not accepted.
- **Verification:** `0.4-conformance.md` maps each accepted artifact to source and test evidence, distinguishes transport-only close errors from HTTP statuses, and records platform/deferred risks. Targeted local tests and the required quality commands are passing; the complete all-targets run is the final gate check before G4 closure.

#### Gate G4 — Operational evidence

- **Status:** DONE — accepted on 2026-09-01.
- **Objective:** Ensure that no production-hardening claim is unsupported or silently exceeds the accepted platform boundary.
- **Deliverables:** Conformance matrix, residual-risk decisions, and reproducible command gate.
- **Dependencies:** Activities 1.1–4.4; optional Activity 4.3 remains excluded.
- **Acceptance criteria:** Each accepted artifact has an executable check, documented registry default, ownership/boundary statement, and residual-risk decision; deferred work is not represented as shipped.
- **Verification:** [`0.4-conformance.md`](0.4-conformance.md), security/threat/concurrency records, local Rust/plugin evidence, and the mandatory quality commands.

## Phase 5 — 0.4 language and BNWeb integration gate

### Async grammar and runtime

#### Activity 5.1 — Align `ASYNC`/`AWAIT` grammar, semantics, IR, and runtime

- **Status:** DONE.
- **Objective:** Implement the separately designed bounded async surface only after the normative 0.4 specification and EBNF agree.
- **Deliverables:** Reserved keywords, AST/source spans for async declarations, parser sugar lowering to explicit `DispatchSubmit`/`DispatchAwait` IR operations backed by the bounded `Dispatch.Queue.Async`/`Ticket.Wait` provider, semantic restrictions, isolated execution, bounded waits, registry-controlled bounded output, cancellation/queue-close behavior, and positive/negative fixtures. LLVM remains explicitly unsupported for these operations.
- **Dependencies:** Activities 0.2 and 0.4; async design; `src/lexer.rs`, `src/parser/`, `src/semantic/`, `src/ir/`, `src/runtime/`.
- **Acceptance criteria:** Async tasks cannot share mutable BN state or output ownership unsafely; waits are bounded to the accepted range; output is written through a bounded writer using `dispatch.output_max_bytes` from the versioned registry; overflow produces a failed ticket with an explicit diagnostic and no partial success; LLVM support is not implied without a separate backend contract.
- **Verification:** The 0.4 lexer table recognizes `ASYNC`/`AWAIT`; `ASYNC FUNCTION` parses, its `asynchronous` metadata survives into IR (`async_function_metadata_survives_into_ir`), `async_submit_and_await_lower_to_explicit_ir_operations` proves explicit `DispatchSubmit`/`DispatchAwait` instructions, and `ASYNC queue Work()`/`AWAIT ticket(1000)` execute through the bounded provider path in `async_await_syntax_dispatches_through_bounded_queue_api`. `ticket_rejects_output_above_registry_bound` and `runtime::executor::part3::tests::task_output_writer_rejects_bytes_after_registry_bound` prove both output boundaries; `async_task_output_overflow_fails_ticket_without_retaining_partial_output` proves overflow becomes status 3 without partial output. `async_function_rejects_non_task_return_type`, `async_submission_rejects_an_invalid_target_operand`, and `await_rejects_literal_timeout_outside_contract` cover negative diagnostics; `bndispatch_ticket_lifecycle_is_bounded` covers cancellation/queue-close. Full async syntax, failure, cancellation, and output-isolation evidence passes.

#### Activity 5.2 — Add opt-in concurrent BNWeb handlers

- **Status:** DONE — accepted on 2026-09-02 after transport, lifecycle, and real BN listener evidence.
- **Objective:** Extend BNWeb only with an explicit bounded queue and isolated handler ownership while preserving the synchronous 0.3 contract.
- **Deliverables:** Registry-backed `ServerOptions.concurrentHandlers` flag (default false), server-owned semaphore sized from the validated worker quota, non-blocking admission with HTTP 503 on handler-slot exhaustion, `spawn_blocking` handler execution with cloned immutable request and private response ownership, active-handler accounting for drain, handler failure/timeout mapping, ordered transport logging, graceful shutdown integration, and migration documentation.
- **Dependencies:** Activities 3.1–3.5 and 4.1–4.2; `BNDispatch`; `ASYNC`/`AWAIT` contract.
- **Acceptance criteria:** The default remains synchronous; opt-in mode has one global bounded handler quota per server; no handler runs without a slot; concurrent handlers cannot share mutable BN request/response state or output writers; one handler owns one response commit; failure, overload, timeout, and stop/drain are explicit and bounded.
- **Verification:** `http::tests::opt_in_concurrent_handler_runs_with_bounded_handler_slot` proves the opt-in path executes the handler off the transport thread and returns its response; `opt_in_concurrent_handler_rejects_when_global_slot_is_occupied` proves a server-owned global slot rejects with 503 without invoking the handler; `opt_in_concurrent_handler_failure_maps_to_internal_error` and `opt_in_concurrent_handler_timeout_maps_to_request_timeout` prove failure/deadline mapping; `concurrent_handler_slots_are_server_owned_and_bounded` and `stop_does_not_claim_success_with_an_active_concurrent_handler` prove quota ownership and drain accounting; `tests/runtime.rs::bnweb_start_serves_registered_bn_handler_over_http` proves a real BN `Server.Start` listener invokes the registered BN handler and projects its response. `runtime::tests::web_callback_uses_a_fresh_executor_and_projects_response` proves request-local interpreter ownership. Full Rust and focused checks pass.

#### Gate G5-A — BN handler bridge ownership decision

- **Status:** DONE — option A accepted and implemented on 2026-09-01; end-to-end evidence remains under Activity 5.2.
- **Objective:** Choose an implementation boundary that permits BN route callbacks to run concurrently without moving mutable executor heaps, raw handles, output writers, or host references across worker threads.
- **Deliverables:** An accepted isolated-executor bridge design and its ownership/marshalling invariants. `Server.Start`/`StartTLS` snapshot route metadata and create a fresh executor per callback; request/response heaps and output writers remain request-local.
- **Dependencies:** Activities 0.4, 3.2–3.3, 5.1, and the current Activity 5.2 transport slice.
- **Acceptance criteria:** The selected design specifies how a fresh executor/module/runtime is built per request, how `Request`/`Response` values are copied or serialized, how filters and handler failures are mapped, how output is bounded, and how stop/drain tracks the task. Capturing the live `Executor` or its object handles is forbidden.
- **Verification:** `runtime::execute_web_callback` and `bn_server_handler` implement the selected ownership boundary; unit compilation/tests pass. The remaining network-level fixture is owned by Activity 5.2 and is required before G5 closes.
- **Deliverables:** Opt-in API, request admission, response ownership, handler timeout/failure mapping, ordered transport logging, graceful shutdown integration, and migration documentation.
- **Dependencies:** Activities 3.1–3.5 and 4.1–4.2; `BNDispatch`; `ASYNC`/`AWAIT` contract.
- **Acceptance criteria:** Default BNWeb remains synchronous; concurrent mode has a finite queue/worker quota, cannot double-commit responses, drains or times out deterministically, and never leaks secrets in access records.
- **Verification:** Local concurrent integration test with overload, handler failure, timeout, stop/drain, response ordering, and log assertions.

## Phase 6 — Developer tooling UX and release integration

### Native debugger integration

#### Activity 6.1 — Make the Rust DAP service event-driven and semantically distinct

- **Status:** DONE — accepted on 2026-09-02 with Rust DAP and VS Code native-bridge evidence.
- **Objective:** Close the gap between the existing interpreter debug hook and a reliable DAP service before exposing it through VS Code.
- **Deliverables:** A DAP event-delivery path that writes queued `stopped`, `continued`, `exited`, and `terminated` events while the client is idle; source paths and source spans in stack frames; a breakpoint response that reports a verified executable line correctly; distinct instruction-level semantics for `next`, `stepIn`, and `stepOut` based on IR instruction boundaries and source spans.
- **Dependencies:** Activity 0.5; `src/dap.rs`; `src/dap/tests.rs`; `src/runtime.rs`; `src/runtime_impl.rs`.
- **Acceptance criteria:** A launch followed by `configurationDone` emits the initial `stopped` event without another client request; `next` advances to a subsequent traceable IR instruction at the current call depth, `stepIn` may enter a called BN function, and `stepOut` resumes until the caller depth is reached. Stack frames identify the originating `.bn` source path. Debugger operations do not evaluate arbitrary user expressions or create a REPL.
- **Verification:** `bn dap` emits `stopped` while idle and the runtime hook carries call depth into explicit `Next`, `In`, and `Out` modes; framing, breakpoint bounds, executable-line mapping, pause/resume, and source/locals projections are covered by Rust tests. The VS Code native bridge smoke test passes and confirms the child lifecycle. Stepping remains explicitly instruction-level with source spans, not a REPL.

#### Activity 6.2 — Wire the VS Code adapter to `bn dap`

- **Status:** DONE — accepted on 2026-09-02 after native bridge and extension checks.
- **Objective:** Replace the launch-only terminal adapter with a local stdio bridge to the native Rust DAP service.
- **Deliverables:** `plugins/vscode/debugAdapter.js` starts the configured `bn dap` process for a debug session and forwards bounded DAP framing bidirectionally; launch configuration continues to supply `program`, `cwd`, and the configured executable; child exit/error handling terminates the VS Code session deterministically. `runInTerminal` remains only for the separate Run command and is absent from debugging.
- **Dependencies:** Activity 6.1; `plugins/vscode/package.json`; `plugins/vscode/debugAdapter.js`; `plugins/vscode/test/debug-adapter.js`.
- **Acceptance criteria:** VS Code receives native DAP `stopped`, `continued`, `exited`, and `terminated` events; breakpoint, continue, pause, step, stack, scopes, and variables requests reach `bn dap`; launch failure is presented as a DAP error; terminating VS Code terminates and reaps the child. No second interpreter, parser, or breakpoint registry exists in JavaScript.
- **Verification:** `node plugins/vscode/test/debug-adapter.js` passes, asserting that no `runInTerminal` request is emitted, the child command is `bn dap`, and the native launch path reaches `stopped`/`continued`/`terminated`; the test also sends `configurationDone` before launch to verify pre-start message queuing. Adapter input/output frames are capped at 1 MiB. `node plugins/vscode/test/test.js` passes extension manifest/language checks.

#### Activity 6.3 — Document source-mapped IR stepping honestly

- **Status:** DONE — accepted on 2026-09-02.
- **Objective:** Set correct user expectations for the debugger without weakening the source-oriented interface.
- **Deliverables:** Updated `plugins/vscode/README.md`, `docs/project/usage.md`, `docs/language/0.4/0.4.md`, and the IDE tooling contract. Documentation states that stepping is over interpreter IR instructions carrying BN source spans; multiple instructions may map to one source line, a line can be revisited by loops, and the debugger is not a REPL or arbitrary-expression evaluator.
- **Dependencies:** Activities 0.2, 6.1, and 6.2.
- **Acceptance criteria:** No document claims terminal launch-only debugging after Activity 6.2; source-level breakpoint rules, supported requests, unavailable compiler/wasm/Jupyter debugging, paused-variable snapshot limits, and instruction-to-line behavior are explicit and consistent.
- **Verification:** `plugins/vscode/README.md`, `docs/project/usage.md`, `docs/language/0.4/0.4.md`, and this WBS describe source-mapped IR-instruction stepping and explicitly reject REPL semantics; repository link/extension checks and the native DAP smoke session pass.

### Notebook execution model

#### Activity 6.4 — Freeze the Jupyter execution-mode contract

- **Status:** DONE — accepted on 2026-09-02 after Program-mode unit and ZeroMQ wire evidence.
- **Objective:** Preserve the safe, current complete-program notebook mode while deciding whether a persistent notebook session belongs in 0.4.
- **Deliverables:** Normative documentation naming the existing mode `Program`: each cell is a complete `.bn` program with `FUNCTION Start()`, executes in a fresh process via `bn run --no-filesystem --jupyter-stdin`, and shares neither declarations nor runtime state. The 0.4 decision is Program-only; persistent `Session` semantics are deferred beyond 0.4.
- **Dependencies:** Activities 0.2, 0.3, and 0.5; `plugins/jupyter/bn_kernel/kernel.py`; `plugins/jupyter/bn_kernel/jupyter.py`; `docs/project/kernel.md`.
- **Acceptance criteria:** Program remains the only supported mode in 0.4 and preserves filesystem denial, interrupt, heartbeat, stdin, and process isolation. The project does not label it a REPL. No Session implementation or documentation is accepted in 0.4 without a new release decision covering declaration/import persistence, value/object ownership, transactional failure, reset/restart, memory/CPU limits, interrupt/cancellation, module reload, debugger availability, and host-capability policy.
- **Verification:** `plugins/jupyter/README.md` freezes Program terminology, complete `Start()` cells, fresh-process isolation, and filesystem denial; `tests/test_kernel.py` passes the complete-program, fresh-process, and capability-denial checks; `tests/test_jupyter.py` passes the kernel-info, execute/IOPub, shutdown, and framing wire fixture using `pyzmq` 27.2.0 in `plugins/jupyter/.venv`. The kernel closes child streams and joins its heartbeat thread during shutdown.

#### Activity 6.5 — Implement optional persistent Jupyter `Session` mode

- **Status:** DEFERRED — explicitly excluded from the 0.4 release by Activity 6.4.
- **Objective:** Provide REPL-like notebook continuity only as an explicit, bounded host mode rather than by concatenating cells or reusing `bn run` process state accidentally.
- **Deliverables:** A separate session host/provider with explicit `Reset`, deterministic cell transaction/rollback behavior, bounded retained declarations/values/output, isolated execution ownership, capability checks before execution, and a precise policy for imports and source locations. Program mode remains available and unchanged.
- **Dependencies:** A future release decision after Activity 6.4 and the accepted async/concurrency policy where required; `plugins/jupyter/`; Rust frontend/interpreter host boundary; Jupyter protocol tests.
- **Acceptance criteria:** A successful session cell exposes only the accepted persistent definitions/values to later cells; a failed cell leaves the previous session unchanged; Reset removes all state; interrupt cannot leave a partially committed session; no session state escapes process/kernel ownership; filesystem and other denied capabilities remain denied before user code.
- **Verification:** Wire and unit tests cover state persistence, no persistence after failure, reset, import reload policy, bounded-memory rejection, interrupt during a transaction, restart, concurrent-client rejection, filesystem denial, and Program-mode regression parity.

### Release gate

#### Activity 6.6 — Close the 0.4 release gate

- **Status:** DONE — accepted on 2026-09-02 after the complete repository/plugin quality gate.
- **Objective:** Accept 0.4 only when specification, implementation, security register, conformance evidence, and integrations agree.
- **Deliverables:** Final WBS/bucket status, updated language and module docs, examples, VS Code/Jupyter plugin updates, release evidence, and residual risk acceptance.
- **Dependencies:** All accepted preceding activities. Activity 4.3 and Activity 6.5 are excluded unless separately accepted.
- **Acceptance criteria:** No unresolved authority conflict, no unowned high/critical finding, all required checks pass, unsupported capabilities are explicit, and plugin versions/examples match the released language contract.
- **Verification:** `cargo test --all-targets -- --test-threads=1`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `git diff --check`, `node plugins/vscode/test/debug-adapter.js`, `node plugins/vscode/test/test.js`, and the Jupyter Program/wire suite pass. No commit, publish, or release operation is part of this WBS.
