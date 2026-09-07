# Basic Next 0.4 BNWeb Threat Model and Resource Policy

This document is the working threat model for the 0.4 BNWeb hardening
program. It is derived from the 0.3 capability model and the active findings
in [`0.4-security-register.md`](0.4-security-register.md). Values marked
proposed require acceptance at the 0.4 authority gate before they become
normative.

## Security boundary

The `.bn` program is application code, not a trusted transport provider. The
native interpreter may expose explicitly imported `HOST.Net`, `HOST.FileSystem`,
`HOST.Console`, `HOST.Random`, and external `BNWeb`/`BNLog` modules. The host
owns OS descriptors, process scheduling, DNS, TLS provider state, filesystem
permissions, and kernel resource limits. BN values must not expose raw
descriptors, private keys, resolver internals, thread handles, or mutable
shared interpreter state.

The 0.4 server boundary is:

```text
BN configuration -> validated immutable options -> listener admission
-> bounded transport work -> canonical request/route/filter pipeline
-> isolated handler/response ownership -> bounded logging -> drain/close
```

Every arrow is a trust or resource boundary. Failure must be explicit and
bounded; no operation may allocate an unbounded collection, wait forever, or
silently grant a host capability.

## Threats and required controls

| ID | Threat | Required control | Evidence |
|---|---|---|---|
| T-01 | SSRF through literal IPv4-mapped IPv6, DNS rebinding, redirects, CGNAT, or special ranges | One classifier for literal, resolved, and redirected destinations; default fail-closed; optional CIDR/port/scheme allowlist | BN-SEC-001/002; local resolver doubles; mapped and CGNAT fixtures |
| T-02 | Connection, thread, task, descriptor, or memory exhaustion | Admission before spawn, finite worker/queue limits, bounded backlog policy, explicit overload result | BN-SEC-004; N+1 controlled-connection test |
| T-03 | Slowloris or stalled TLS handshake | Header, body, idle, total, and TLS handshake deadlines | BN-SEC-005/006; slow-peer and TLS fixtures |
| T-04 | Session takeover | At least 128 bits of CSPRNG output, rotation invalidation, secure cookie attributes, no token logs | BN-SEC-003/006; session and cookie tests |
| T-05 | Response/browser policy weakness | Typed strict security-header profile; HTTPS-only HSTS; bounded valid headers | BN-SEC-006; HTTP/1.1, HTTP/2, TLS tests |
| T-06 | Log injection or secret disclosure | Mandatory case-insensitive redaction, control escaping, bounded records, no body/query/cookie/TLS-key fields | BN-SEC-007; format/transport matrix |
| T-07 | Stop race or resource leak | Stop accepting first, signal, drain, join, deadline, and explicit timeout state | BN-SEC-005; lifecycle tests |
| T-08 | Trusted-proxy spoofing | Use forwarded client identity only for explicitly trusted proxies; otherwise use transport peer | BN-SEC-002/007; proxy provenance tests |
| T-09 | Async/shared-state corruption | Explicit queue, isolated module/runtime ownership, no shared mutable BN objects or output writers | 0.4 async design; worker-isolation tests |
| T-10 | Capability confusion or downgrade | Explicit imports, provider checks before execution, no TLS-to-cleartext fallback, deterministic unavailable errors | 0.3 contract; capability and TLS tests |

## Accepted 0.4 defaults and bounds

The machine-readable registry at
[`config/0.4-bnweb-limits.toml`](../config/0.4-bnweb-limits.toml) is the single
versioned source for these values. Hosts may configure a lower value but may
not raise a value beyond the maximum without a new contract decision.

The implementation must load this registry through one typed configuration
layer, validate units and cross-field relationships before bind/start, and
expose the validated immutable result to both `Start` and `StartTLS`. Numeric
limits must not be copied as independent literals into `http.rs`, `web.rs`,
`part4.rs`, or test-only production paths. A registry change requires updated
configuration-parity tests and an explicit review of affected acceptance
criteria.

| Resource or policy | Proposed default | Allowed range / maximum | Exhaustion or invalid result |
|---|---:|---:|---|
| Active server connections | 128 | 1–256 | Close before response, or `503` when a response is possible |
| OS listen backlog | 128 | Provider-defined, capped at 128 | Document provider result; never claim exact backlog if unavailable |
| Pending transport/handler work | 128 | 1–512 | `503 Service Unavailable`; handler is not invoked |
| Worker count | 8 | 1–64 | Configuration error before bind |
| Header section | 64 KiB / 100 fields | Existing 0.3 maximum | `431 Request Header Fields Too Large` or typed client error |
| Request/response body | 8 MiB | Existing 0.3 maximum | `413 Payload Too Large` or typed client error |
| TLS handshake | 5 seconds | 1–60,000 ms | Close socket; emit bounded error record |
| Header-read deadline | 5 seconds | 1–60,000 ms | `408 Request Timeout` when possible |
| Body-read deadline | 30 seconds | 1–60,000 ms | `408 Request Timeout` |
| Idle keep-alive deadline | 60 seconds | 1–300,000 ms | Close idle connection |
| Complete connection deadline | 120 seconds | 1–600,000 ms | Close/typed timeout; no handler continuation |
| Stop/drain deadline | 5 seconds | 1–60,000 ms | Typed stop-timeout result; do not claim drained |
| Redirects | 10 | 0–10 | Typed redirect-limit error |
| Rate-limit key table | 10,000 keys | 1–10,000 | Deterministic eviction; never grow table |
| Request ID | 16 random bytes | Exactly one bounded encoded token | Generate a new valid ID or reject per API policy |
| Session ID | 16 random bytes minimum | No upper unbounded value | CSPRNG/provider error; no session insertion |
| Log record | Existing BNLog bound | Existing accepted maximum | Redact/truncate/reject according to BNLog policy |

`429 Too Many Requests` is reserved for an accepted rate-limit policy and may
include a bounded `Retry-After`. `503 Service Unavailable` represents server
admission or queue overload. The two cases must not be conflated.

## Lifecycle invariants

1. A server validates all options before binding or exposing a listener.
2. Admission occurs before a connection creates a task, thread, or runtime.
3. The listener enters `Draining` before any connection/task join begins.
4. A successful `Stop`/`Close` owns and accounts for every listener and
   admitted connection; no detached worker remains.
5. A deadline expiry returns an explicit timeout result and preserves enough
   state to prevent a later call from reporting a false successful drain.
6. A rate-limited or overloaded request never invokes BN filters or handlers.
7. A failed TLS reload leaves the previous complete certificate/key pair live.
8. Security logging never changes a committed response and never logs request
   body, query values, cookies, session IDs, authorization, or private keys.

## Capability and deployment assumptions

- No public Internet, external DNS authority, external certificate authority,
  or external metrics service is required for conformance.
- OS backlog, file-descriptor, thread, and process limits remain deployment
  responsibilities and must be recorded when platform evidence is claimed.
- `HOST.FileSystem` remains explicitly imported and provider-gated; Jupyter
  retains its `--no-filesystem` boundary.
- Cleartext and TLS use the same admission, timeout, lifecycle, and logging
  invariants. TLS never falls back to cleartext.
- The 0.4 async/concurrent design cannot weaken the synchronous 0.3 contract;
  concurrent BNWeb handling is opt-in and uses isolated bounded work.

## Acceptance decision

**Accepted — 2026-09-01.** Carlos approved the defaults and bounds in the
table above. They are normative for the 0.4 BNWeb hardening implementation;
implementations must reject values outside the stated range, preserve the
distinction between `429` and `503`, and retain the lifecycle invariants.
Platform-specific provider limits remain deployment evidence, not permission
to exceed these maxima.

## Acceptance gate

Activity 0.3 is complete after the acceptance decision above and the mapping
of every threat to an implementation activity and local executable test. The
remaining implementation evidence is tracked by Activities 0.4–4.4; this
document is now the accepted threat-model baseline.
