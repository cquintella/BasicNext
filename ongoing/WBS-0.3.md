# Basic Next 0.3

## Phase 0 — Contract and dependency gate

### API freeze

#### Activity 0.1 — Freeze `HOST.Net`, `BNJson`, `BNLog`, and `BNWeb` contracts

- **Status:** DONE.
- **Sprint:** 0.
- **Objective:** define every public type, method, error channel, ownership rule, limit, and unsupported-host outcome.
- **Deliverables:** accepted `0.3.ebnf`, `0.3.md`, keyword registry,
  [`host-net.md`](../docs/language/0.3/host-net.md),
  [`bnlog.md`](../docs/language/0.3/bnlog.md),
  [`bnweb.md`](../docs/language/0.3/bnweb.md), `modules/bn/BNLog.bn`,
  `modules/bn/BNWeb.bn`, capability/module signature tables, and resolved D-005
  and D-019 entries in [`0.3-decisions.md`](0.3-decisions.md).
- **Dependencies:** BDFL decision on the `HOST.Net`, `BNJson`, `BNLog`, and `BNWeb` APIs.
- **Acceptance criteria:** no public API is ambiguous; `HOST.Net` is
  native-interpreter-only; `BNLog` and `BNWeb` have explicit imports; the
  dependency graph is acyclic; `0.3.md` changes from active draft to accepted.
- **Verification:** cross-check the accepted signatures against grammar, semantics, IR, and module declarations.

#### Activity 0.2 — Approve the runtime dependency set

- **Status:** DONE.
- **Sprint:** 1.
- **Objective:** select minimal JSON, HTTP/1.1, HTTP/2, TLS, URL, cookie, ICMP,
  and HTML parser dependencies; QUIC/HTTP/3 remain outside the 0.3 graph.
- **Deliverables:** updated [`0.3-decisions.md`](0.3-decisions.md) with exact
  versions, direct/transitive licenses, enabled features, target/MSRV evidence,
  native-code consequences, and security review.
- **Dependencies:** Activities 0.1 and 0.3.
- **Acceptance criteria:** no production dependency is added before approval; the standard library is used for sockets and system resolution.
- **Verification:** review `Cargo.lock`, `cargo tree -e features`, target builds,
  and approved license/advisory/source checks.

### Security and resource policy

#### Activity 0.3 — Freeze the threat model and operational limits

- **Status:** DONE.
- **Sprint:** 0.
- **Objective:** bound network, logging, HTTP, session, scraping, tooling, and shutdown
  resources before accepting untrusted input.
- **Deliverables:** accepted byte/count/queue/time limits, SSRF and trusted-proxy
  policy, overload behavior, and typed limit/timeout errors.
- **Dependencies:** Activity 0.1 and the proposed limits in
  [`0.3-decisions.md`](0.3-decisions.md).
- **Acceptance criteria:** every wait, queue, input, output, expansion, and
  retained state has a default bound and a documented owner.
- **Verification:** threat-model review maps each trust boundary to an
  acceptance test and a failure result; bounded HOST.Net, BNLog, and runtime
  negative-path tests provide executable evidence.

## Phase 1 — Language and identity foundation

### Frontend support

#### Activity 1.1 — Implement multi-binding `LET`

- **Status:** DONE.
- **Sprint:** 2.
- **Objective:** implement the accepted multi-binding declaration grammar and semantics.
- **Deliverables:** lexer/parser/AST/semantic/IR/interpreter support and positive/negative fixtures.
- **Dependencies:** Activity 0.1.
- **Acceptance criteria:** one type applies to all names; initializer arity
  matches binding arity; all declared names are out of scope in all
  initializers; evaluation and assignment are left to right.
- **Verification:** grammar, semantic, runtime, and diagnostic-span tests.

#### Activity 1.2 — Implement single-line `IF`

- **Status:** DONE.
- **Sprint:** 2.
- **Objective:** accept `IF condition THEN statement` with an optional
  single-line `ELSE statement` while preserving the existing block form.
- **Deliverables:** parser/AST/span, semantic, IR, interpreter, diagnostic, and
  0.3 fixture support for the accepted grammar.
- **Dependencies:** Activity 0.1.
- **Acceptance criteria:** each branch contains exactly one simple statement;
  compact `ELSE` remains on the same physical line; no `END IF` follows;
  `THEN NEWLINE` always requires block `END IF`; nested/compound compact
  statements are rejected; the condition, narrowing, return analysis, and
  runtime behavior match block `IF`.
- **Verification:** positive/negative planning fixtures migrate to the grammar
  suite; true/false/else, equality, declaration scope, return, trailing comment,
  rejected `END IF`, rejected nested `IF`, and diagnostic-span tests pass.

#### Activity 1.3 — Add host and external-module identities

- **Status:** DONE.
- **Sprint:** 2.
- **Objective:** make `HOST.Net`, `BNLog`, and `BNWeb` resolved identities rather than name-based special cases.
- **Deliverables:** semantic identities, typed IR entries, native capability/provider registration, and unsupported-host diagnostics.
- **Dependencies:** Activity 0.1.
- **Acceptance criteria:** user declarations cannot forge the identities; compiler, Jupyter, and wasm reject unavailable imports before `Start`.
- **Verification:** import, identity, alias, collision, and target-rejection fixtures.

## Phase 2 — `HOST.Net`

### Network values and system integration

#### Activity 2.1 — Implement address and endpoint values

- **Status:** DONE.
- **Sprint:** 3.
- **Objective:** provide typed IPv4/IPv6 addresses, CIDRs, endpoints, and system forward/reverse resolution.
- **Deliverables:** semantic types, native interpreter values, CIDR containment logic, and injected resolver tests.
- **Dependencies:** Activities 0.1, 0.3, and 1.3.
- **Acceptance criteria:** address parsing and endpoints use `std::net`; CIDR
  uses only the approved Phase 0 choice; no network protocol is reimplemented;
  user classes cannot forge values.
- **Verification:** IPv4/IPv6 parsing, bounds, CIDR, forward/reverse, and provider-failure tests.

#### Activity 2.2 — Implement bounded TCP and UDP

- **Status:** DONE.
- **Sprint:** 3.
- **Objective:** provide TCP connect/bind/listen/accept and UDP send/receive without exposed descriptors.
- **Deliverables:** bounded I/O, EOF, timeout, close, endpoint, and IPv4/IPv6 listener behavior.
- **Dependencies:** Activity 2.1.
- **Acceptance criteria:** all reads, writes, queues, and timeouts are bounded; listeners start all-or-nothing; close is idempotent.
- **Verification:** local IPv4/IPv6 loopback, timeout, EOF, overflow, close, and negative-path tests without live Internet services.

#### Activity 2.3 — Implement bounded ICMP Echo

- **Status:** DONE.
- **Sprint:** 4.
- **Objective:** provide one-shot IPv4/IPv6 `Ping` without exposing raw packet
  construction or requiring the rest of `HOST.Net` to share ICMP privileges.
- **Deliverables:** typed Ping reply/error values, per-host provider, bounded
  timeout/concurrency, and capability-matrix evidence.
- **Dependencies:** Activities 0.1, 0.2, 0.3, and 2.1, plus resolution of D-016.
- **Acceptance criteria:** one call sends one fixed host-generated Echo payload;
  missing permission disables only Ping; no subprocess, raw handle, arbitrary
  ICMP, spoofing, traceroute, or packet capture is exposed.
- **Verification:** local IPv4/IPv6 loopback, timeout, unreachable,
  permission-denied, unavailable-provider, concurrency-limit, and identifier
  correlation tests. On the current macOS target, deterministic unavailable
  behavior is verified; no unsupported Linux/Windows claim is made.

#### Activity 2.4 — Implement direct-neighbor lookup

- **Status:** DONE.
- **Sprint:** 4.
- **Objective:** expose safe ARP/NDP lookup for directly connected peers.
- **Deliverables:** platform-specific provider and deterministic unavailable/error behavior.
- **Dependencies:** Activity 2.1.
- **Acceptance criteria:** no raw packet or neighbor-table mutation is exposed.
- **Verification:** deterministic unavailable behavior and host capability
  evidence are recorded for the current macOS target.

#### Activity 2.5 — Record transparent IPsec host evidence

- **Status:** DONE.
- **Sprint:** 4.
- **Objective:** document transparent operating-system IPsec use by `HOST.Net` without creating a BN IPsec API.
- **Deliverables:** native capability-matrix entries and executable deployment evidence for each claimed host.
- **Dependencies:** Activity 2.2 and host-specific IPsec policy configured outside BN.
- **Acceptance criteria:** BN exposes no key, IKE, tunnel, security association, or kernel-policy value; unsupported hosts make no IPsec claim.
- **Verification:** current macOS target makes no IPsec support claim and
  exposes no IPsec API; ordinary HOST.Net TCP/UDP evidence remains valid.

## Phase 3 — Logging and `BNWeb` application model

### Structured logging and transport-neutral server pipeline

#### Activity 3.1 — Implement `BNLog`

- **Status:** DONE.
- **Sprint:** 5.
- **Objective:** implement bounded structured logging with Winston-style
  separation of levels, formats, and transports.
- **Deliverables:** `modules/bn/BNLog.bn`, logger/entry/fields identities,
  JSON Lines/text/Apache formats, console/file/null transports, child context,
  flush, and idempotent close.
- **Dependencies:** Activities 0.1, 0.2, 0.3, and 1.3.
- **Acceptance criteria:** dispatch is synchronous and ordered; every eligible
  transport is attempted; the first failure is reported; no dependency on
  `BNWeb` or `HOST.Net` exists.
- **Verification:** level, format, structured-field, child-context,
  multi-transport, partial-failure, flush, close, append, and bound tests. The
  formatter core plus explicit-capability append-only file and console
  transports, ordered multi-transport failure propagation, and bounded flush/
  close synchronization are implemented.

#### Activity 3.2 — Add the `BNWeb` module and request pipeline

- **Status:** DONE.
- **Sprint:** 6.
- **Objective:** implement typed `BNWeb` declarations and a transport-neutral request/response pipeline.
- **Deliverables:** `modules/bn/BNWeb.bn`, resolved module identity, bounded
  request/response values, URL validation/canonicalization, ordered typed
  filters, trusted-proxy provenance, and deterministic literal/`:name` routes.
  The native `Server` lifecycle now owns a bounded `ServerState`; route
  registration and start/stop/close validation are wired through the runtime.
  Native `Response` objects now enforce status/header/body bounds and commit
  state, and `Request` owns bounded method/target/body values. Native opaque
  header/query collections expose bounded `Count`/`Get` operations. The Hyper
  adapter maps accepted `HOST.Net` streams into bounded `Request` values, and
  `Server.Start` owns a bounded accept loop over the supplied endpoint.
  Explicit synchronous `Server.Dispatch` now executes the registered filters
  and route handler with shared `Request`/`Response` objects and emits a
  bounded HTTP access record through a configured logger for explicit
  synchronous dispatch. HTTP transport
  The transport now supports an explicit synchronous Rust handler callback that
  projects bounded status, headers, and body; BN interpreter callback projection
  is deferred to the 0.4 `BNWeb` revision by accepted D-023. HTTP transport access
  logging is deferred to 0.4 by accepted D-025. Provenance is explicit and
  safe by default: `PeerAddress` is the socket peer and
  `EffectiveClientAddress` ignores forwarding metadata unless a future
  trusted-proxy policy is explicitly configured.
- **Dependencies:** Activities 0.1, 0.2, 0.3, 1.3, 2.2, and 3.1.
- **Acceptance criteria:** deterministic route selection; automatic `404`/`405`;
  malformed or ambiguous URLs are rejected before matching; decoding occurs
  exactly once; filters cannot rewrite the canonical route identity;
  every resolver/socket operation uses the resolved `HOST.Net` provider;
  explicit synchronous dispatch records use the configured `BNLog` logger;
  transport access logs are deferred by D-025; no generic
  middleware/plugin system.
- **Verification:** provider-identity, invalid percent/UTF-8/control/dot/encoded
  separator, duplicate-query, route-ordering, response-commit, filter, proxy,
  logging, limit, and handler-failure tests.

#### Activity 3.3 — Implement server lifecycle

- **Status:** DONE.
- **Sprint:** 6.
- **Objective:** define bounded queues, synchronous serial handlers, overload behavior, graceful stop, and cleanup.
- **Deliverables:** native interpreter server lifecycle implementation. The
  bounded `ServerState` queue, overload result, graceful stop, and close state
  machine plus synchronous transport-neutral route dispatch callback are
  implemented in `src/web.rs`; transport accept is now wired through the
  Hyper adapter, while HTTP-to-BN callback wiring is deferred to the 0.4 `BNWeb`
  revision by accepted D-023. Runtime route
  registration now rejects non-function handlers before
  mutating the route table, and server lifecycle failures stay in the declared
  `VOID OR Error` channel. Explicit `Server.Dispatch` now invokes bounded
  filters and the selected synchronous handler.
- **Dependencies:** Activity 3.2.
- **Acceptance criteria:** no async BN syntax or thread-safety claim; all queues and waits are bounded.
- **Verification:** overload, timeout, stop, cleanup, and local-client tests.

## Phase 4 — HTTP transports and security

### Client and server protocols

#### Activity 4.1 — Implement HTTP/1.1 and HTTP/2

- **Status:** DONE.
- **Sprint:** 7.
- **Objective:** map HTTP/1.1 and HTTP/2 into the shared pipeline using the approved dependency.
- **Deliverables:** client/server protocol adapters with header/body limits,
  cancellation, and authority validation. Hyper auto-detection over accepted
  `HOST.Net` streams is implemented for a local server connection; `Server.Start`
  wires the bounded accept loop and maps method, canonical target, headers, peer
  address, and bounded UTF-8 body into the shared request pipeline. Local
  HTTP/1.1 and HTTP/2 route tests now exercise the same status semantics; request
  body collection has a bounded 10-second deadline;
  `OPTIONS` returns `204` with `Allow`; an explicit Rust handler callback now
  projects committed response status, headers, and body. BN transport callback
  projection is deferred to 0.4/BNThreads by accepted D-023.
- **Dependencies:** Activities 0.2, 2.2, and 3.3.
- **Acceptance criteria:** equivalent route behavior over both protocols; the
  protocol engine receives connections/connectors from `HOST.Net`; no parallel
  socket provider and no private HTTP framing parser exist.
- **Verification:** local cleartext keep-alive, multiplexing, malformed-framing, slow-client, and limit tests.

#### Activity 4.2 — Implement HTTPS server and bounded transport policy

- **Status:** DONE.
- **Sprint:** 7.
- **Objective:** add the accepted 0.3 HTTPS server/TLS boundary and bounded transport policy. Rustls server configuration, ALPN, strict bounded PEM validation, and compressed-response rejection are wired; HTTPS client transport, trust roots, redirects, and client write-timeout behavior are deferred to 0.4 by D-026.
- **Deliverables:** HTTPS server adapter and local certificate fixtures; the HTTPS client is explicitly a 0.4 deliverable.
- **Dependencies:** Activity 4.1.
- **Acceptance criteria:** no cleartext downgrade on the server; TLS/ALPN and bounded certificate policy are verified. No HTTPS client support is claimed in 0.3.
- **Verification:** IPv4/IPv6 local TLS, invalid certificate, ALPN, decompression, and timeout tests; deferred client checks are tracked in the 0.4 plan.

## Phase 5 — Stateful and audited web features

### Web state and observability

#### Activity 5.1 — Implement cookies, sessions, and static scraping

- **Status:** DONE.
- **Sprint:** 8.
- **Objective:** provide isolated cookie jars, bounded in-memory sessions, and non-executing HTML scraping.
- **Deliverables:** cookie/session stores, approved HTML parser integration, CSS selectors, and bounded extraction APIs.
- **Dependencies:** Activities 0.2 and 4.2.
- **Acceptance criteria:** state never crosses unrelated clients; scripts, event handlers, and subresources never execute.
- **Verification:** domain/path/expiry, session rotation/eviction, charset, size/work ceiling, and non-execution tests.

#### Activity 5.2 — Implement ACL and Apache logs

- **Status:** DONE.
- **Sprint:** 8.
- **Objective:** enforce deterministic access policy and auditable request logging without bundling external geolocation data.
- **Deliverables:** ordered default-deny ACL and escaped Apache Combined-compatible records emitted through `BNLog`; geolocation is explicitly removed from the 0.3 support claim by D-008.
- **Dependencies:** Activity 4.2 and the optional-data gate in
  [`0.3-decisions.md`](0.3-decisions.md).
- **Acceptance criteria:** provenance distinguishes peer, trusted effective origin, HTTP `Origin`, and local destination; log write failures are reported; no geolocation dataset is bundled or downloaded.
- **Verification:** IPv4/IPv6 ACL, proxy provenance, log-injection, Apache bounds, and rejected-exchange tests.

## Phase 6 — IDE tooling

### Native protocol services

#### Activity 6.1 — Implement native LSP

- **Status:** DONE.
- **Sprint:** 9.
- **Objective:** provide local, real-time diagnostics, definition/references, and completion from the shared frontend.
- **Deliverables:** stdio LSP service, bounded full-document model, lifecycle,
  lexer/parser/semantic diagnostics, semantic navigation/completion, and VS
  Code client integration. The initial stdio lifecycle, full-sync diagnostics,
  AST declaration navigation, token references, and keyword completion slice is
  implemented in `src/lsp.rs`; definition lookup follows explicitly imported
  matching open documents and bounded sibling filesystem modules (`file://`,
  8 MiB cap). The VS Code extension starts `bn lsp` and forwards document
  lifecycle events, diagnostics, definition, and completion requests.
- **Dependencies:** Activity 0.2, Activities 1.1 and 1.2, and the accepted
  [`ide-tooling.md`](../docs/language/0.3/ide-tooling.md) LSP 3.18 wire contract.
- **Acceptance criteria:** incomplete-source diagnostics and source spans agree with the shared frontend; no second parser or semantic analyzer exists.
- **Verification:** local LSP unit tests for UTF-16/range conversion, definitions,
  references, completion, bounded filesystem imports, `node --check` and VS Code
  extension checks, plus the full Rust quality gate.

#### Activity 6.2 — Implement native DAP

- **Status:** DONE.
- **Sprint:** 10.
- **Objective:** provide native-interpreter launch, breakpoints, stepping, stack frames, and local inspection.
- **Deliverables:** stdio DAP service, interpreter debug hooks, executable-span mapping, and VS Code debug integration. The bounded Content-Length framing, lifecycle ordering, bounded `.bn` launch/module-graph validation through frontend load/semantic/IR lowering, stateful line-validated `setBreakpoints` registry, AST statement-span executable-line mapping, and protocol-shaped responses are implemented in `src/dap.rs`; the public `runtime::DebugHook` and `execute_with_host_debug` expose read-only instruction-span callbacks, and `execute_with_host_debug_control` now drives a threaded session with continue, pause, and step commands plus stopped/continued/terminated events. Stack frames are source-span snapshots with read-only symbol/value snapshots exposed through DAP scopes and variables.
- **Dependencies:** Activity 6.1 and the accepted
  [`ide-tooling.md`](../docs/language/0.3/ide-tooling.md) DAP subset.
- **Acceptance criteria:** paused inspection never executes user code; non-executable breakpoints are rejected explicitly; wasm, compiler, and Jupyter remain unavailable without providers.
- **Verification:** local tests for framing, breakpoint mapping, threaded pause/resume,
  step control, source-span stack frames, read-only variable snapshots,
  termination, negative paths, and VS Code adapter checks.

## Phase 7 — Conformance and release

### Release gate

#### Activity 7.1 — Complete conformance and integrations

- **Status:** DONE.
- **Sprint:** 11.
- **Objective:** establish executable evidence for the accepted 0.3 surface.
- **Deliverables:** grammar/semantic/IR/runtime/protocol matrices, examples, plugin updates, dependency inventory, capability matrix, and release evidence. The executable network example is [`examples/socket.bn`](../examples/socket.bn): one argument-driven TCP/UDP client-server program with IPv4/IPv6 loopback coverage and server-side `BNLog` JSON Lines records.
- **Dependencies:** Activities 1.1 through 6.2 and inherited Windows TTY evidence from 0.2.
- **Acceptance criteria:** no live Internet service is required; every support claim has executable evidence; Jupyter remains unavailable until a real provider exists.
- **Verification:** [`0.3-conformance.md`](0.3-conformance.md),
  `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`,
  `git diff --check`, `socket.bn --help`, local IPv4/IPv6 TCP/UDP request-reply
  tests with `BNLog` output, module checks, plugin
  checks, documented Jupyter provider gating, and inherited Windows console
  evidence.

#### Activity 7.2 — Document the language, HOST, and external modules in the book

- **Status:** DONE.
- **Sprint:** 11.
- **Objective:** make the accepted 0.3 behavior discoverable in `docs/book` without conflating the language core with provider-backed modules.
- **Deliverables:** update the main book chapters for the 0.3 core; add a dedicated `HOST` chapter near standard-library usage; create separate appendices for `BNJson`, `BNLog`, `BNWeb`, `BNData`, and the external-module conventions; update the table of contents and cross-links.
- **Dependencies:** Activities 1.1 through 6.2, D-024, and the accepted 0.3 module contracts.
- **Acceptance criteria:** every documented signature and example matches the normative 0.3 specification; `HOST` is clearly distinguished from external `BN*` modules; appendices do not imply that BN modules are built into the core.
- **Verification:** book link/path audit, separate external-module appendices,
  dedicated HOST chapter, runnable `.bn` examples where available, all module
  `bn check` fixtures, and review against `docs/language/0.3/0.3.md`,
  `0.3.ebnf`, and `keywords.md`.

## Phase 8 — Compiler and runtime modularization

### Refactoring and optimization gate

#### Activity 8.0 — Split god modules into phase-local components

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** enforce the low-level Rust modularity contract before concurrent runtime work: every Rust file stays at or below 520 lines, state dependencies are narrow, and the compiler pipeline remains strictly forward-only.
- **Deliverables:** `runtime/` modules for values, numeric operations, execution, memory, HOST, and external providers; `semantic/`, `ir/`, `parser/`, and `llvm/` modules divided by phase responsibility; smaller CLI/web/protocol modules; an updated component map; and zero strict-Clippy warnings.
- **Dependencies:** accepted 0.3 behavior, completed Task 3 LLVM slice, and the revised `rust-low-level-development` skill.
- **Definition of ready:** module boundaries are responsibility-based; public APIs and diagnostic codes are frozen; no language behavior change is included; focused regression tests exist for every extracted slice.
- **Implementation directives:** keep `Source -> Tokens -> AST -> Semantic Analysis -> IR`; lower phases never import higher phases; `lib.rs` only declares/reexports modules; avoid context bags; pass narrow state; preserve spans; use checked conversions/allocation sizes and `?`; add no dependency or `unsafe`; keep each new file at or below 520 lines.
- **Acceptance criteria:** every `src/**/*.rs` and `src/*.rs` file is at most 520 lines; parser performs no type checking; analyzer owns types/symbols; IR and LLVM remain downstream; all pre-refactor observable tests retain their outcomes.
- **Definition of done:** all oversized files are split, component documentation is current, focused and full tests pass, strict Clippy passes, formatting and diff checks pass, and review evidence is recorded.
- **Verification:** `wc -l src/**/*.rs src/*.rs`, `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `git diff --check`, plus focused parser/semantic/IR/runtime/codegen/protocol suites after each extraction.

#### Activity 8.0.1 — Extract the runtime numeric slice

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** establish the first vertical refactoring slice by moving numeric evaluation, conversions, ranges, parsing, and rendering out of `runtime.rs` without changing runtime behavior.
- **Deliverables:** responsibility-focused runtime numeric, rendering, collection/index, allocation, and temporal submodules under 520 lines, narrow `pub(super)` interfaces, preserved `NUMERIC_OVERFLOW` behavior, and focused arithmetic/vector tests.
- **Dependencies:** Activity 8.0.
- **Acceptance criteria:** numeric functions depend only on `Value`, semantic numeric types, spans, and diagnostics; no provider or executor state is imported; checked integer and vector allocation behavior is unchanged.
- **Verification:** focused overflow/vector/index/temporal runtime tests, targeted `cargo fmt`, and `cargo clippy --all-targets -- -D warnings` pass; `src/runtime/numeric.rs`, `src/runtime/render.rs`, `src/runtime/collections.rs`, `src/runtime/allocation.rs`, and `src/runtime/temporal_ops.rs` are below 520 lines.

#### Activity 8.0.1.1 — Extract runtime lookup and comparison helpers

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move constant/value lookup, arity validation, default-value construction, type predicates, and structural equality into focused runtime modules while keeping the executor API unchanged.
- **Deliverables:** `runtime/helpers.rs` and `runtime/compare.rs`, each below 520 lines, with narrow `pub(super)` interfaces and preserved diagnostic behavior.
- **Dependencies:** Activity 8.0.1.
- **Acceptance criteria:** runtime dispatch uses the extracted helpers; comparison semantics for numeric, record, vector, object, pointer, temporal, and external values remain unchanged; no new dependency or unsafe code is introduced.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the full no-default-features test suite pass except the known sandbox-denied socket integration test (`Operation not permitted`).

#### Activity 8.0.1.2 — Route semantic type operations through a phase-local module

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate binary typing, conversion admission, numeric promotion, and integer-width rules in `semantic/types.rs`, keeping semantic analysis as the sole owner of these rules.
- **Deliverables:** `semantic/types.rs` with narrow `pub(super)` operations and analyzer call sites routed through it; no parser, IR, or runtime dependency is introduced.
- **Dependencies:** Activity 8.0.1.1.
- **Acceptance criteria:** arithmetic, comparison, assignment, conversion, shift-count, float-width, and integer-promotion behavior is unchanged; diagnostics retain their source spans and codes.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass; the known socket integration remains sandbox-denied only.

#### Activity 8.0.1.3 — Isolate AST declaration-to-type conversion

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate declaration kind, function signature, type reference, and type atom conversion in `semantic/declarations.rs` while preserving vector dimensions and imported names.
- **Deliverables:** `semantic/declarations.rs` with narrow analyzer-facing functions and unchanged declaration/type behavior.
- **Dependencies:** Activity 8.0.1.2.
- **Acceptance criteria:** `FUNCTION`, `POINTER`, scalar, vector, imported, and declaration type mappings retain their existing semantic results; no parser or IR dependency is introduced.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.4 — Move signature and pointer type parsing

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** keep function-signature token parsing, qualified names, pointer element extraction, and pointer length interpretation with declaration/type conversion.
- **Deliverables:** `semantic/declarations.rs` owns the moved helpers; `semantic.rs` retains only analyzer-specific allocation and expression rules.
- **Dependencies:** Activity 8.0.1.3.
- **Acceptance criteria:** function, pointer, qualified-name, and vector declaration behavior remains unchanged and source diagnostics remain intact.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.5 — Centralize scalar type and literal utilities

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** centralize scalar type-name parsing, integer/float limits, literal folding, and default literal typing in `semantic/types.rs`.
- **Deliverables:** active analyzer imports from `semantic/types.rs`; no duplicate scalar parser remains in `semantic.rs`.
- **Dependencies:** Activity 8.0.1.4.
- **Acceptance criteria:** scalar aliases (`INTEGER`, `FLOAT`, `FLOAT32`, `FLOAT64`), base/radix integer literals, range checks, constant folding, and default literal types are unchanged.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.6 — Centralize static type sizing utilities

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move static length, static size, integer byte width, and checked dimension-product calculations into `semantic/types.rs` while preserving the public semantic API consumed by IR and runtime.
- **Deliverables:** type sizing implementation in `semantic/types.rs`; stable public wrappers in `semantic.rs`; no duplicated sizing implementation remains.
- **Dependencies:** Activity 8.0.1.5.
- **Acceptance criteria:** scalar/vector size and length results, unknown-dimension handling, overflow behavior, and public callers remain unchanged.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.7 — Remove runtime comparison duplication

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** remove dead equality and type-predicate implementations from `runtime.rs` after routing all execution paths through `runtime/compare.rs`.
- **Deliverables:** one active implementation for runtime equality and type predicates; reduced executor source size.
- **Dependencies:** Activity 8.0.1.6.
- **Acceptance criteria:** numeric, record, vector, object, pointer, temporal, and external-value comparisons retain their prior outcomes without dead-code allowances.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.8 — Extract BNMath vector reductions

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate vector and pointer statistical reductions from the runtime executor into `runtime/math.rs`.
- **Deliverables:** focused math module below 520 lines with checked numeric conversion and preserved diagnostics.
- **Dependencies:** Activity 8.0.1.7.
- **Acceptance criteria:** `MIN`, `MAX`, `MEAN`, `MEDIAN`, quartiles, mode, range, variance, and standard deviation retain existing behavior for vectors and pointer regions.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.9 — Extract HOST.Net value conversion

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate address-list validation, address/endpoint parsing, and runtime record construction for HOST.Net.
- **Deliverables:** `runtime/net_values.rs` below 520 lines with checked port/address conversions and narrow runtime interfaces.
- **Dependencies:** Activity 8.0.1.8.
- **Acceptance criteria:** IPv4/IPv6 parsing, endpoint validation, address/endpoint record shapes, and diagnostic codes remain unchanged.
- **Verification:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, library/binary tests, and `git diff --check` pass.

#### Activity 8.0.1.10 — Extract IR validation and CFG builder

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate IR structural validation and CFG builder operations in `ir/validate.rs` while keeping `ir::validate` public and preserving lowering call sites.
- **Deliverables:** phase-local IR validation module, explicit imports, stable public wrapper, and no pipeline reversal.
- **Dependencies:** Activity 8.0.1.9.
- **Acceptance criteria:** dangling blocks/values remain rejected, builder-generated CFG and terminators are unchanged, and IR remains downstream of semantic analysis.
- **Verification:** `cargo test --test ir --no-default-features`, library/binary tests, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` pass.

#### Activity 8.0.1.11 — Split IR validation from CFG builder

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move the `Builder` implementation into `ir/builder.rs`, keeping `ir/validate.rs` limited to structural module checks.
- **Deliverables:** `ir/builder.rs` with explicit phase-local imports and `ir/validate.rs` with focused validation logic.
- **Dependencies:** Activity 8.0.1.10.
- **Acceptance criteria:** lowering callers retain access to builder methods, public `ir::validate` behavior is unchanged, and no source-to-IR phase dependency is reversed.
- **Verification:** IR integration tests, library/binary tests, strict Clippy, formatting, and diff checks pass; the next gate will split the builder itself below 520 lines.

#### Activity 8.0.1.12 — Extract IR builder state primitives

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate block creation, value allocation, instruction emission, terminator management, open-block checks, and finalization from the IR lowering builder.
- **Deliverables:** `ir/builder/builder_state.rs` (67 lines) with explicit crate-visible methods; builder lowering behavior unchanged.
- **Dependencies:** Activity 8.0.1.11.
- **Acceptance criteria:** block/value IDs remain deterministic, terminators are enforced, and all existing IR lowering tests retain their results.
- **Verification:** `cargo test --test ir --no-default-features`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` pass.

#### Activity 8.0.1.13 — Extract IR statement lowering

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate statement-level lowering (bindings, assignments, control effects, prints, and deletes) from expression and CFG lowering.
- **Deliverables:** `ir/builder/statements.rs` (208 lines) with explicit builder implementation and preserved source spans.
- **Dependencies:** Activity 8.0.1.12.
- **Acceptance criteria:** statement lowering emits the same IR instructions and diagnostics; expression lowering remains separate and downstream of semantic analysis.
- **Verification:** `cargo test --test ir --no-default-features`, strict Clippy, formatting, and `git diff --check` pass.

#### Activity 8.0.1.14 — Extract IR expression lowering

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate expression lowering from statement and CFG state code in `ir/builder/expressions.rs`.
- **Deliverables:** expression module with preserved recursive lowering and source spans; builder state remains separate.
- **Dependencies:** Activity 8.0.1.13.
- **Acceptance criteria:** literals, names, calls, casts, vectors, indexing, members, and short-circuit entry points retain their IR output.
- **Verification:** IR suite, strict Clippy, formatting, and diff checks pass; `ir/builder.rs` is 395 lines, `expressions.rs` 394 lines, and `control_flow.rs` 337 lines.

#### Activity 8.0.1.15 — Close the IR builder size gate

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** ensure all IR builder components remain at or below 520 lines after extracting state, statements, expressions, and control flow.
- **Deliverables:** `ir/builder.rs`, `ir/builder/builder_state.rs`, `ir/builder/statements.rs`, `ir/builder/expressions.rs`, and `ir/builder/control_flow.rs` all below the size limit.
- **Dependencies:** Activity 8.0.1.14.
- **Acceptance criteria:** lowering output and public APIs remain unchanged; the Source → Tokens → AST → Semantic Analysis → IR pipeline remains forward-only.
- **Verification:** `cargo test --test ir --no-default-features`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `git diff --check`, and line-count audit pass.

#### Activity 8.0.1.16 — Isolate networking test fixtures

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move networking unit fixtures out of the production networking module so its implementation remains below 520 lines.
- **Deliverables:** `src/net/tests.rs` and a focused `#[cfg(test)] mod tests` gateway in `net.rs`.
- **Dependencies:** Activity 8.0.1.15.
- **Acceptance criteria:** IPv4/IPv6 addressing, CIDR, resolver, TCP, and UDP tests retain their behavior and production `net.rs` is below 520 lines.
- **Verification:** `cargo test --lib net --no-default-features`, `cargo fmt --check`, and line-count audit pass.

#### Activity 8.0.1.17 — Isolate HTTP adapter test fixtures

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move HTTP adapter fixtures out of the production HTTP module while retaining protocol and bounds coverage.
- **Deliverables:** `src/http/tests.rs` and a focused `#[cfg(test)] mod tests` gateway in `http.rs`.
- **Dependencies:** Activity 8.0.1.16.
- **Acceptance criteria:** HTTP/1.1, HTTP/2, TLS, callback, routing, redirect, and request-bound tests retain their behavior and production `http.rs` is below 520 lines.
- **Verification:** `cargo test --lib http --no-default-features`, `cargo fmt --check`, and line-count audit pass.

#### Activity 8.0.1.18 — Isolate LSP test fixtures

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move LSP protocol fixtures out of the production service module and keep `lsp.rs` below 520 lines.
- **Deliverables:** `src/lsp/tests.rs` and the test gateway in `lsp.rs`.
- **Dependencies:** Activity 8.0.1.17.
- **Acceptance criteria:** navigation, definition, references, completion, UTF-16 position, and bounded sibling-module tests retain their behavior.
- **Verification:** `cargo test --lib lsp --no-default-features`, `cargo fmt --check`, and line-count audit pass.

#### Activity 8.0.1.19 — Isolate DAP test fixtures

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move DAP protocol and execution-session fixtures out of the production adapter module and keep `dap.rs` below 520 lines.
- **Deliverables:** `src/dap/tests.rs` and the test gateway in `dap.rs`.
- **Dependencies:** Activity 8.0.1.18.
- **Acceptance criteria:** framing, launch validation, breakpoint registry, executable-line mapping, and pause/resume tests retain their behavior.
- **Verification:** `cargo test --lib dap --no-default-features`, `cargo fmt --check`, and line-count audit pass.

#### Activity 8.0.1.20 — Isolate BNWeb test fixtures

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move BNWeb protocol and lifecycle fixtures out of the production web module as a prerequisite for splitting its request, routing, and server-state responsibilities.
- **Deliverables:** `src/web/tests.rs` and the test gateway in `web.rs`.
- **Dependencies:** Activity 8.0.1.19.
- **Acceptance criteria:** route precedence, canonicalization, request/response bounds, proxy provenance, lifecycle, SSRF, and collection tests retain their behavior.
- **Verification:** `cargo test --lib web --no-default-features`, `cargo fmt --check`, and line-count audit pass.

#### Activity 8.0.1.21 — Split runtime executor responsibilities

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** decompose the runtime executor into provider- and operation-focused modules without changing the public runtime facade or interpreter semantics.
- **Deliverables:** `runtime/executor/part*.rs`, terminal capability module, helper module, and explicit provider boundaries.
- **Dependencies:** Activities 8.0.1.1–8.0.1.20.
- **Acceptance criteria:** all runtime tests and strict Clippy remain green; every resulting Rust source file is below 520 lines, including the large web, host, and data-frame dispatch methods.
- **Verification:** focused runtime tests, full library tests, strict Clippy, formatting, diff checks, and line-count audit.

#### Activity 8.0.1.22 — Isolate runtime host/support utilities

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move clock, random-seed, and debugger-variable support plus runtime fixtures behind focused modules while keeping the runtime facade below 520 lines.
- **Deliverables:** `src/runtime/support.rs`, `src/runtime/tests.rs`, and stable runtime imports.
- **Dependencies:** Activity 8.0.1.21.
- **Acceptance criteria:** fixed/system clocks, random seed, debugger snapshots, and runtime tests retain behavior; `runtime_impl.rs` is below 520 lines.
- **Verification:** `cargo test --lib --no-default-features`, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.23 — Split DataFrame executor branches

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** extract DataFrame joins, append operations, selection/slicing, column creation, and count queries from the central dispatcher.
- **Deliverables:** focused DataFrame methods in `runtime/executor/part12.rs` and `part13.rs`; `part9.rs` below 520 lines.
- **Dependencies:** Activity 8.0.1.22.
- **Acceptance criteria:** DataFrame row/column counts, joins, append, selection, slicing, conversion, statistics, and pointer-copy behavior remain unchanged.
- **Verification:** `cargo test --lib --no-default-features`, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.24 — Split HOST.Net executor branches

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate HOST.Net address, resolver, TCP, UDP, and packet operations from clock, random, console, and filesystem dispatch.
- **Deliverables:** `runtime/executor/part14.rs` and a reduced host dispatcher.
- **Dependencies:** Activity 8.0.1.23.
- **Acceptance criteria:** all HOST.Net operations retain bounded IPv4/IPv6, TCP, UDP, timeout, and capability behavior; every resulting source file is below 520 lines.
- **Verification:** networking/runtime focused tests, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.25 — Isolate HOST.Net address operations

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move address, endpoint, CIDR, and host reachability branches into a focused executor module.
- **Deliverables:** `runtime/executor/part15.rs`; reduced TCP/UDP dispatcher in `part14.rs`.
- **Dependencies:** Activity 8.0.1.24.
- **Acceptance criteria:** address parsing, endpoint construction, CIDR operations, ping/neighbor/reverse capability behavior remain unchanged.
- **Verification:** networking-focused tests, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.26 — Isolate HOST.Net TCP operations

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** move TCP listener, stream, and resolver branches into a focused executor module.
- **Deliverables:** `runtime/executor/part16.rs`; reduced UDP/packet/address collection dispatcher in `part14.rs`.
- **Dependencies:** Activity 8.0.1.25.
- **Acceptance criteria:** TCP/UDP socket semantics, bounded timeouts, EOF, close, resolver limits, and IPv4/IPv6 behavior remain unchanged.
- **Verification:** `cargo test --lib --no-default-features`, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.27 — Split BNWeb runtime provider dispatch

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** separate BNWeb state, request, and response operations from the remaining client/server dispatcher.
- **Deliverables:** `runtime/executor/part17.rs`, `part18.rs`, and `part19.rs`; reduced `part4.rs`.
- **Dependencies:** Activity 8.0.1.26.
- **Acceptance criteria:** session, scraper, ACL, cookie, TLS, header/query, request, and response semantics remain unchanged; `part4.rs` is below 520 lines.
- **Verification:** library tests, strict Clippy, formatting, and line-count audit.

#### Activity 8.0.1.28 — Split CLI frontend and BNWeb routing state

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate command-line frontend parsing/loading and BNWeb route matching/server lifecycle from their facades.
- **Deliverables:** `cli_frontend.rs`, `web/routing.rs`, and `web/server.rs`; production `web.rs` remains below 520 lines.
- **Dependencies:** Activity 8.0.1.27.
- **Acceptance criteria:** CLI option parsing, route selection, method negotiation, and server lifecycle behavior remain unchanged.
- **Verification:** `cargo fmt --check`, `cargo test --lib --no-default-features`, and line-count audit.

#### Activity 8.0.1.29 — Split parser phases

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate precedence, primary-expression, declaration, statement, and token utility parsing into phase-local modules.
- **Deliverables:** `parser/expressions.rs` and `parser/phase1.rs` through `phase4.rs`; parser facade retains all public parse entry points and source-span diagnostics.
- **Dependencies:** Activity 8.0.1.28.
- **Acceptance criteria:** expression parsing behavior and diagnostics remain unchanged, with the extracted module below 520 lines.
- **Verification:** `cargo fmt --check`, `cargo test --lib --no-default-features`, and line-count audit.

#### Activity 8.0.1.30 — Split IR lowering helpers

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** separate module/function lowering and IR utility routines from the public IR facade.
- **Deliverables:** `ir/lowering.rs`, `ir/lowering_callable.rs`, and `ir/helpers.rs`; public `lower`, `lower_graph`, and `validate` APIs remain stable.
- **Dependencies:** Activity 8.0.1.29.
- **Acceptance criteria:** IR lowering behavior remains unchanged and every extracted file stays below 520 lines.
- **Verification:** `cargo fmt --check`, `cargo test --lib --no-default-features`, strict Clippy, and diff checks.

#### Activity 8.0.1.31 — Split LLVM lowering phases

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** separate LLVM analysis, instruction emission, terminator/cast handling, and utility routines while preserving typed CFG lowering.
- **Deliverables:** `llvm/analysis.rs`, `llvm/emission1.rs`, `llvm/emission2.rs`, `llvm/emission3.rs`, `llvm/emission_tail.rs`, and `llvm/helpers.rs`; `llvm.rs` remains the public facade.
- **Dependencies:** Activity 8.0.1.30.
- **Acceptance criteria:** generated IR and unsupported-lowering diagnostics remain unchanged; every extracted LLVM file is below 520 lines.
- **Verification:** `cargo fmt --check`, `cargo test --lib --no-default-features`, strict Clippy, and diff checks.

#### Activity 8.0.1.32 — Split semantic module analysis

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate module graph analysis and import/interface validation from the semantic facade.
- **Deliverables:** `semantic/module_analysis.rs`; stable `analyze` and `analyze_modules` behavior.
- **Dependencies:** Activity 8.0.1.31.
- **Acceptance criteria:** module diagnostics and imported-type resolution remain unchanged; extracted code is below 520 lines.
- **Verification:** library tests, strict Clippy, formatting, and diff checks.

#### Activity 8.0.1.33 — Split semantic analyzer phases and HOST defaults

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** divide analyzer responsibilities into phase-local modules and isolate standard HOST declarations.
- **Deliverables:** `semantic/analyzer1.rs` through `analyzer8.rs` and `semantic/host_defaults.rs`.
- **Dependencies:** Activity 8.0.1.32.
- **Acceptance criteria:** semantic state, diagnostics, and public APIs remain stable; every Rust file remains at or below 520 lines.
- **Verification:** `cargo test --lib --no-default-features`, strict Clippy, formatting, and diff checks.

#### Activity 8.0.1.34 — Split semantic type operations

- **Status:** DONE.
- **Sprint:** 12.
- **Objective:** isolate scalar type helpers, numeric operations, and qualified/pointer type-name handling.
- **Deliverables:** `semantic/helpers*.rs`, `semantic/type_ops.rs`, and `semantic/type_names.rs`; stable public size/type wrappers.
- **Dependencies:** Activity 8.0.1.33.
- **Acceptance criteria:** declaration, conversion, numeric typing, and pointer-shape behavior remain unchanged; strict visibility has no warnings.
- **Verification:** `cargo test --lib --no-default-features`, strict Clippy, formatting, and diff checks.

## Phase 9 — Bounded dispatch and deferred web scope

### Dispatch foundation

#### Activity 8.1 — Freeze the `BNDispatch` execution and resource contract

- **Status:** DONE.
- **Sprint:** 13.
- **Objective:** backport the bounded dispatch scope from the 0.4 forward plan as the explicit external `BNDispatch` module, while preserving `HOST` as the only built-in interface object.
- **Deliverables:** accepted `BNDispatch` interface for serial/concurrent queues, named-function task submission, D-028 tickets, join/group, queue barriers, participant barriers, mutexes, semaphores, lifecycle/timeout/error rules, and the `BNWeb` worker ownership model.
- **Dependencies:** D-027 and BDFL decisions on cross-worker BN execution, task argument binding, cancellation, queue ordering, and failure propagation.
- **Definition of ready:** every public operation has fixed ownership, limits, timeout, shutdown, and error behavior; no closure or user-object transfer semantics are implicit.
- **Acceptance criteria:** `BNDispatch` remains an explicit import; workers are bounded; no OS thread handle, unsafe shared interpreter state, or unbounded wait is exposed.
- **Verification:** API/module review, D-028/D-029 decision records, module identity tests, bounded queue/ticket tests, and a threat-model check for task ownership and deadlock behavior.

#### Activity 8.2 — Implement `HOST.NumProcs`

- **Status:** DONE.
- **Sprint:** 13.
- **Objective:** expose the logical processor count available to the current process so bounded dispatch can select a host-aware worker limit.
- **Deliverables:** `HOST.NumProcs() AS INTEGER OR Error` in semantic analysis, IR lowering, interpreter runtime, normative language/keyword/book documentation, and runtime coverage.
- **Dependencies:** D-027.
- **Acceptance criteria:** no import is needed; the count is positive and represents logical processors available to the process, not a physical-core claim; an unavailable or out-of-range host result is `Error`.
- **Verification:** `host_num_procs_reports_a_positive_logical_processor_count`, `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `git diff --check`.

#### Activity 8.3 — Implement bounded queues and synchronization

- **Status:** DONE.
- **Sprint:** 13.
- **Objective:** provide the accepted first vertical slice of `BNDispatch` over a bounded native worker pool.
- **Deliverables:** module interface and Rust provider for queue creation, named-function submission, tickets/join, groups, queue and participant barriers, semaphores, mutexes, close/cancellation behavior, and deterministic errors.
- **Dependencies:** Activity 8.1 and the accepted execution model.
- **Definition of done:** focused local queue, join, barrier, timeout, cancellation, and resource-limit tests; runnable BN examples; documentation; repository Rust checks.
- **Acceptance criteria:** a queue never creates one worker per task; failures unblock dependent joins; barriers cannot strand waiters after timeout or close; shared state has an explicit synchronization owner.
- **Verification:** `bndispatch_queue_rejects_worker_count_outside_the_bounded_range`, `bndispatch_ticket_lifecycle_is_bounded`, `bndispatch_wait_executes_named_task_and_completes_ticket`, `bndispatch_sync_primitives_have_bounded_operations`, `concurrent_queue_runs_two_jobs_at_once`, and native queue saturation tests; full `cargo test`, strict Clippy, formatting, and diff checks. Local only, with no Internet service.

### Deferred BNWeb 0.4 transport scope

#### Activity 9.1 — Bind `BNWeb` transport work to `BNDispatch`

- **Status:** DEFERRED TO 0.4.
- **Sprint:** 0.4.
- **Objective:** replace the 0.3 deferred transport-to-BN boundary with an opt-in, bounded web worker model.
- **Deliverables:** explicit queue configuration, request ownership/isolation, bounded admission, graceful stop, and local HTTP/1.1/HTTP/2 callback evidence, owned by `ongoing/0.4-forward-plan.md`.
- **Dependencies:** a 0.4 concurrency and ownership decision gate.
- **Acceptance criteria:** no concurrent access to one interpreter state is implied; request failure/timeout/stop behavior is deterministic; the 0.3 serial path remains unchanged.
- **Verification:** deferred to the 0.4 plan; no 0.3 verification is required or claimed.

#### Activity 9.2 — Complete deferred `BNWeb` logging and HTTPS client boundaries

- **Status:** DEFERRED TO 0.4.
- **Sprint:** 0.4.
- **Objective:** deliver the deferred ordered transport access-log sink and HTTPS client with an explicit trust-root policy.
- **Deliverables:** bounded ordered `BNLog` transport sink, flush/failure behavior, HTTPS client API, approved trust-root provider, local certificate fixtures, redirects, and timeout/body-limit policy, owned by `ongoing/0.4-forward-plan.md`.
- **Dependencies:** the 0.4 web-worker model and a BDFL trust-root decision.
- **Acceptance criteria:** 0.3 makes no transport-access-log or HTTPS-client support claim; the 0.4 design must prevent indefinite blocking, exclude secrets/bodies by default, and never silently downgrade HTTPS.
- **Verification:** deferred to the 0.4 plan; no 0.3 verification is required or claimed.

## Phase 10 — Vector and LLVM contract alignment

### Contract and lowering follow-up

#### Activity 10.1 — Align declaration-time vectors and LLVM lowering with the accepted 0.3 contract

- **Status:** DONE.
- **Sprint:** 15.
- **Objective:** align local declaration-time vector dimensions, typed LLVM lowering, explicit CFG emission, and resilient IR test construction with the accepted 0.3 contract.
- **Deliverables:** local-binding dimension-expression support across parser, semantics, IR, runtime, and grammar fixtures; typed LLVM numeric mappings; explicit block/terminator lowering with deterministic unsupported-operation diagnostics; `Module::default()`-based IR tests; and updated normative language/release-tracking documents.
- **Dependencies:** Activities 1.1, 7.2, 8.3, and the accepted 0.3 vector/LLVM contract design.
- **Acceptance criteria:** local `LET` bindings may evaluate a non-negative integer dimension expression exactly once while signatures, fields, parameters, and return types remain literal-only; LLVM lowering never executes BN programs to synthesize output; unsupported lowering reports `BUILD_LOWERING_UNAVAILABLE`; overflow diagnostics remain aligned across interpreter and backend.
- **Verification:** focused parser, semantic, runtime, IR, codegen, metadata, and repository checks pass; full repository verification is recorded at G7.

#### Activity 10.2 — Gate G7: accept vector and LLVM contract alignment evidence

- **Status:** DONE.
- **Sprint:** 15.
- **Objective:** close the vector/LLVM follow-up only after the accepted contract, implementation, and release evidence agree.
- **Deliverables:** recorded outcomes for the accepted parser/semantic/runtime/IR/codegen/repository commands, plus residual unsupported-lowering notes.
- **Dependencies:** Activity 10.1.
- **Acceptance criteria:** grammar, normative prose, keyword registry, implementation, and evidence files make one consistent support claim with no silent widening of the accepted 0.3 boundary.
- **Verification:** parser, semantic, runtime, IR, codegen, metadata, formatting, Clippy, full tests, and diff checks pass; unsupported lowering remains explicit as `BUILD_LOWERING_UNAVAILABLE`.
