# Basic Next Security Register (BN-SEC)

This is the canonical, living security register for the Basic Next project.
Tracked findings and residual risks are maintained here with living evidence.

## Authority & Policies

- Every finding remains tracked until verified by automated tests and living documentation.
- Residual risk must explicitly name an Owner and a Defer-until target date or milestone.
- Security tests must use local fixtures, deterministic clocks, and fake resolvers; no external internet dependency is permitted.

## BN-SEC Register

| id | title | status | evidence_path | owner | updated |
| --- | --- | --- | --- | --- | --- |
| BN-SEC-001 | IPv4-mapped IPv6 SSRF classification | Fixed | `src/web/tests.rs::ssrf_guard_reclassifies_ipv4_mapped_ipv6_as_ipv4` | Doug | 2026-09-07 |
| BN-SEC-002 | Incomplete special IPv4-range policy | Fixed | `src/web/tests.rs::ssrf_guard_rejects_special_ipv4_ranges_but_allows_global_ipv4` | Doug | 2026-09-07 |
| BN-SEC-003 | Predictable session ID generation | Fixed | `src/web_state.rs::tests::session_ids_are_random_and_have_at_least_128_bits` | Doug | 2026-09-07 |
| BN-SEC-004 | Unbounded per-connection thread creation | Fixed | `src/web/tests.rs::server_admission_bounds_connections_before_worker_spawn` | Doug | 2026-09-07 |
| BN-SEC-005 | Incomplete graceful stop and connection tracking | Fixed | `src/web/tests.rs::server_stop_drains_a_slow_http_worker` | Doug | 2026-09-07 |
| BN-SEC-006 | Response security and cookie policy | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g3-cookies.md` | Doug | 2026-09-07 |
| BN-SEC-007 | Log redaction and observability leakage | Fixed | `src/log.rs` / format tests | Doug | 2026-09-07 |

## Audit 2026-09-07 Findings Disposition

| id | title | status | evidence_path | owner | updated |
| --- | --- | --- | --- | --- | --- |
| F-01 | Egress multi-A connect-to-allowlisted-only | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g1-f01.md` | Doug | 2026-09-07 |
| F-02 | POLICY_CLOCK / FILESYSTEM / RANDOM vs bn_rt | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g2a-policy.md` | Doug | 2026-09-07 |
| F-03 | CookieJar / Set-Cookie HTTP | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g3-cookies.md` | Doug | 2026-09-07 |
| F-04 | HOST.Random non-CSPRNG documentation | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g4a-random.md` | Doug | 2026-09-07 |
| F-05 | FS default sandbox & rooted TOCTOU | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g4b-fs.md` | Doug | 2026-09-09 |
| F-06 | DataFrame FFI bounds and safety | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g2b-ffi.md` | Doug | 2026-09-07 |
| F-07 | 503 on admit failure | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-07 |
| F-08 | Living threat model and BN-SEC register | Fixed | `docs/security/threat-model.md`, `docs/security/security-register.md` | Doug | 2026-09-07 |
| F-09 | macOS keychain PEM | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-07 |
| F-10 | arp/ndp PATH | Fixed | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-07 |

## Residuals

| id | title | status | evidence_path | owner | updated |
| --- | --- | --- | --- | --- | --- |
| R-01 | SSRF DNS rebinding and redirect limits | Residual | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-09 |
| R-02 | Session store process-local lifetime | Residual | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-09 |
| R-03 | OS file descriptor / socket backlog limits | Residual | `docs/superpowers/evidence/2026-09-07-0.4.6-g5-register.md` | Doug | 2026-09-09 |
