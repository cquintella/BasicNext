# Basic Next 0.3 Release Bucket

This is an interactive live file to show the AGILE development process for version 0.3 of Basic Next.

Release news: [`0.3-release-news.md`](0.3-release-news.md) summarizes the
rebuilt implementation, completed gates, and explicit 0.4 boundaries.

Authority, in order: [`0.3.ebnf`](../docs/language/0.3/0.3.ebnf),
[`0.3.md`](../docs/language/0.3/0.3.md), and
[`keywords.md`](../docs/language/0.3/keywords.md).

Execution is sequential. Only the first sprint containing an unchecked
activity is active; a later sprint starts only after the preceding sprint and
its decision gate are complete. Detailed objectives, deliverables,
dependencies, acceptance criteria, and checks live in
[`WBS-0.3.md`](WBS-0.3.md). Planning decisions and dependency evidence live in
[`0.3-decisions.md`](0.3-decisions.md).

**Current execution point:** SPRINT 15 — COMPLETE. Sprint duration is set at kickoff from
available capacity; scope and exit evidence do not move between sprints.
The verified Sprint 11 boundary remains serial `Server.Dispatch`, explicit
dispatch access records, and the HTTPS server/TLS adapter. Sprint 12 is the
mandatory modularization/strict-Clippy gate introduced by the revised Rust
engineering contract. Sprint 13 completed `BNDispatch`. Sprint 14 is deferred
to 0.4; the concurrent `BNWeb` boundary remains outside the 0.3 support claim.

## SECTION 1 — Contract, risk, and dependencies

Freeze behavior before code or production dependencies.

### SPRINT 0 — API and threat-model gate

- [X] ACTIVITY 0.1 — Freeze `HOST.Net`, `BNJson`, `BNLog`, and `BNWeb` contracts.
- [X] ACTIVITY 0.3 — Freeze the threat model and operational limits.
- [X] DECISION D-005 — Freeze public signatures, ownership, errors, and defaults.
- [X] DECISION D-006 — Accept or replace synchronous serial BN handler dispatch.
- [X] DECISION D-014 — Resolve per-transport host-capability requirements for `BNLog`.
- [X] DECISION D-019 — Freeze route conflict precedence.
- [X] GATE G0 — Accept public signatures, ownership, errors, limits, and trust boundaries.

Exit evidence: accepted signature tables and limits in `host-net.md`,
`bnlog.md`, and `bnweb.md`; no implementation-defined behavior remains.

### SPRINT 1 — Dependency gate

- [X] ACTIVITY 0.2 — Approve exact dependencies, features, licenses, targets, and security evidence.
- [X] DECISION D-004 — Accept standard-library-first `HOST.Net` and the CIDR boundary.
- [X] DECISION D-009 — Accept the shared JSON/LSP and minimal DAP dependency policy.
- [X] DECISION D-010 — Select and audit the Rustls cryptographic provider.
- [X] DECISION D-016 — Select ICMP providers and per-host unavailable behavior.
- [X] GATE G1 — Accept the locked dependency graph and Rustls cryptographic provider.

Exit evidence: reviewed lock graph, feature tree, licenses, advisories, MSRV,
native-code consequences, and target builds.

## SECTION 2 — Language and provider identities

Implement the smallest language amendment and secure the native identities.

### SPRINT 2 — Language amendments and provider identity

- [X] ACTIVITY 1.1 — Implement multi-binding `LET` end to end.
- [X] ACTIVITY 1.2 — Implement single-line `IF` end to end.
- [X] ACTIVITY 1.3 — Add unforgeable `HOST.Net`, `BNLog`, and `BNWeb` identities.

Exit evidence: grammar, semantic, IR, runtime, negative, and diagnostic-span
tests pass for both syntax changes and all three identities.

## SECTION 3 — Native networking

Build bounded host networking on operating-system services.

### SPRINT 3 — Addressing and sockets

- [X] ACTIVITY 2.1 — Implement address, CIDR, endpoint, and resolver values.
- [X] ACTIVITY 2.2 — Implement bounded TCP and UDP.

Exit evidence: deterministic local IPv4/IPv6 resolver, CIDR, TCP, UDP,
timeout, EOF, truncation, close, and bound tests pass without Internet access.

### SPRINT 4 — Host-specific network evidence

- [X] ACTIVITY 2.3 — Implement bounded IPv4/IPv6 ICMP Echo where supported.
- [X] ACTIVITY 2.4 — Implement safe direct-neighbor lookup where supported.
- [X] ACTIVITY 2.5 — Record transparent operating-system IPsec evidence.

Exit evidence: ICMP and neighbor availability are recorded per host; every
IPsec claim has host-controlled executable evidence and exposes no BN IPsec API.

## SECTION 4 — Logging and web application model

Establish structured logging before the transport-neutral BN request pipeline.

### SPRINT 5 — Structured logging

- [X] ACTIVITY 3.1 — Implement `BNLog` levels, fields, formats, transports, flush, and close.

Exit evidence: ordered multi-transport, structured JSON/text, bounds,
redaction, partial failure, flush, close, and capability-availability tests pass.

### SPRINT 6 — Routes and lifecycle

- [X] ACTIVITY 3.2 — Add `BNWeb` over `HOST.Net`, with bounded request/response values, logging, filters, provenance, and routes. (Core pipeline, explicit synchronous BN dispatch with bounded `BNLog` access records, and transport-neutral Rust handler response projection implemented; HTTP-to-BN projection is deferred by accepted D-023; HTTP transport access logs are deferred by accepted D-025.)
- [X] ACTIVITY 3.3 — Implement bounded dispatch, overload, stop, and cleanup. (State machine, bounded lifecycle, and synchronous BN dispatch implemented; transport callback projection is deferred by accepted D-023.)

Exit evidence: URL rejection/canonicalization, route precedence, filters,
provenance, explicit-dispatch access logging, overload, stop, and cleanup pass
over local `HOST.Net` provider connections; transport access logging is covered
by the 0.4 forward plan.

## SECTION 5 — HTTP transports and security

Deliver HTTP/1.1, HTTP/2, and TLS. HTTP/3 is deferred to 0.4.

### SPRINT 7 — HTTP baseline

- [X] ACTIVITY 4.1 — Implement HTTP/1.1 and HTTP/2 adapters. (Local protocol adapters, parity tests including HEAD body suppression, bounded request deadline, strict invalid-header rejection, automatic 404/405 response bodies, and synchronous Rust handler response projection across cleartext/TLS adapters implemented; BN transport callback projection is deferred by accepted D-023.)
- [X] ACTIVITY 4.2 — Implement HTTPS server and bounded transport policy. (TLSConfig API, StartTLS wiring, strict bounded PEM/Base64 validation, compressed-response rejection, and server-side limits landed; HTTPS client transport, trust roots, redirects, and client write-timeout matrix are deferred by accepted D-026.)
- [X] GATE G2 — Accept local HTTP/TLS evidence for the accepted 0.3 boundary; transport-to-BN callbacks, transport access logs, and HTTPS client support are explicitly deferred by D-023/D-025/D-026.

Exit evidence: HTTP/1.1 and HTTP/2 have identical application semantics; TLS,
ALPN, certificate, decompression, and slow-client server tests pass. No HTTPS
client or HTTP/3 support claim is present in the 0.3 graph.

## SECTION 6 — Stateful web facilities

Add bounded state and audit features without a browser or bundled datasets.

### SPRINT 8 — State, scraping, policy, and audit

- [X] ACTIVITY 5.1 — Implement cookie jars, sessions, and static scraping. (Cookie domain/subdomain/path matching including root paths and max-age deletion, session create/get/set/rotate/delete with idle expiry pruning, bounded 30-minute maximum idle timeout, and oldest-entry eviction, bounded tag-selector scraping, script exclusion, static factories, and BN runtime integration tests pass.)
- [X] ACTIVITY 5.2 — Implement ACL and Apache-compatible logs. (ACL provider and Apache Combined formatter implemented and verified; geolocation is removed from the 0.3 support claim by accepted D-008 and tracked for a future version.)
- [X] DECISION D-008 — Remove geolocation from the 0.3 support claim; no MMDB provider or dataset is included.
- [X] GATE G3 — Confirm that no geolocation dataset is bundled or downloaded.

Exit evidence: cookie isolation, session rotation/expiry/eviction, static
non-execution, ACL/provenance, and log escaping pass; geolocation is not a 0.3
support claim.

## SECTION 7 — IDE tooling

Reuse the existing frontend and interpreter through published local protocols.

### SPRINT 9 — Language server

- [X] ACTIVITY 6.1 — Implement the native LSP 3.18 subset and update VS Code integration. (stdio diagnostics/navigation/completion and VS Code extension checks pass; definition lookup follows explicitly imported matching open documents and bounded sibling `file://` modules capped at 8 MiB; the extension starts `bn lsp` and forwards lifecycle events.)

Exit evidence: stdio lifecycle, document replacement, diagnostics, definition,
references, completion, UTF-16 positions, and `bn check` span parity pass.

### SPRINT 10 — Debug adapter

- [X] ACTIVITY 6.2 — Implement the native DAP subset and interpreter debug hooks. (stateful lifecycle ordering, bounded `.bn` launch/module-graph validation through load/semantic/IR lowering, stateful `setBreakpoints` registry with deduplication/line validation, AST statement-span executable-line mapping, threaded runtime continue/pause/step control with DAP stopped/continued/terminated events, source-span stack frames, read-only symbol/value snapshots, and VS Code adapter checks pass.)

Exit evidence: framing, launch, breakpoint mapping, stepping, stack/scopes,
inspection without user-code execution, termination, and VS Code tests pass.

## SECTION 8 — Release evidence

Make every support statement executable and reproducible.

### SPRINT 11 — Conformance and integrations

- [X] ACTIVITY 7.1 — Complete conformance matrices, examples, plugins, capability evidence, and release checks. (All `modules/bn/*.bn` files pass `bn check`; [`examples/socket.bn`](../examples/socket.bn) covers `--help`, TCP/UDP, client/server, IPv4/IPv6 loopback, and server `BNLog` output; [`examples/icmp-ping.bn`](../examples/icmp-ping.bn) is a manual diagnostic for `www.intelliurb.com` and is not release evidence because current macOS `Ping` is deliberately operation-unavailable under D-016; VS Code and Jupyter manifests are at 0.3; VS Code grammar/extension/debug-adapter checks pass; Jupyter wire checks record the unavailable `pyzmq` provider; inherited Windows evidence is preserved.)
- [X] ACTIVITY 7.2 — Document the 0.3 language and external modules in `docs/book`. (Core language concepts remain in main chapters; `BNJson`, `BNLog`, `BNWeb`, `BNData`, and external-module conventions are in separate appendices; `HOST` has a dedicated chapter near standard-library usage.)
- [X] GATE G4 — Accept 0.3 only with local executable evidence for every claimed feature. (All WBS activities are DONE; Rust, module, IPv4/IPv6 socket example, VS Code, DAP, documentation, and capability evidence are recorded in [`0.3-conformance.md`](0.3-conformance.md); Jupyter remains explicitly provider-gated and Windows console evidence is inherited from 0.2.)

Exit evidence: all WBS activities are DONE; conformance/capability matrices,
examples, dependency inventory, plugins, Windows evidence, and release checks
are complete.

## SECTION 9 — Compiler/runtime modularization

Remove the god-module bottleneck before adding concurrent execution paths.

### SPRINT 12 — Refactoring and optimization gate

- [X] ACTIVITY 8.0 — Split every Rust source file above 520 lines into responsibility-focused modules while preserving public APIs and the forward-only compiler pipeline. Sprint 12 status updated after the completed modularization, strict checks, and 517-line maximum audit.
- [X] ACTIVITY 8.0.1 — Extract runtime numeric operations (`numeric.rs`, 164 lines), value rendering (`render.rs`, 53 lines), collection/index helpers (`collections.rs`, 67 lines), allocation helpers (`allocation.rs`), temporal providers (`temporal_ops.rs`), runtime lookup helpers (`helpers.rs`, 110 lines), and value/type comparison (`compare.rs`, 196 lines); focused runtime tests and strict Clippy pass; remaining runtime extraction stays in this sprint.
- [X] ACTIVITY 8.0.1.1 — Route semantic binary typing, conversion checks, and numeric promotion through `semantic/types.rs`; preserve the existing analyzer contract and pass strict Clippy/tests.
- [X] ACTIVITY 8.0.1.2 — Route AST declaration/reference-to-type conversion through `semantic/declarations.rs`; preserve vector-dimension and imported-type semantics with strict checks.
- [X] ACTIVITY 8.0.1.3 — Move signature parsing, token-to-type conversion, qualified names, and pointer shape helpers into `semantic/declarations.rs`; focused checks remain green.
- [X] ACTIVITY 8.0.1.4 — Move scalar type-name parsing, integer/float limits, literal folding, and default literal typing into `semantic/types.rs`; preserve diagnostics and analyzer behavior.
- [X] ACTIVITY 8.0.1.5 — Move static length/size, scalar byte widths, and checked dimension products into `semantic/types.rs`, retaining public semantic API wrappers.
- [X] ACTIVITY 8.0.1.6 — Remove legacy runtime equality/type-predicate implementations after routing all call sites through `runtime/compare.rs`; strict checks pass.
- [X] ACTIVITY 8.0.1.7 — Extract BNMath vector reductions into `runtime/math.rs`; preserve numeric behavior and pass strict checks.
- [X] ACTIVITY 8.0.1.8 — Extract network address/endpoint conversion helpers into `runtime/net_values.rs`; preserve HOST.Net behavior and pass strict checks.
- [X] ACTIVITY 8.0.1.9 — Move IR structural validation and builder implementation into `ir/validate.rs`, preserving public `ir::validate` and lowering callers.
- [X] ACTIVITY 8.0.1.10 — Separate IR builder implementation into `ir/builder.rs`, leaving structural validation focused; all IR and strict checks pass.
- [X] ACTIVITY 8.0.1.11 — Extract builder state/emission primitives into `ir/builder/builder_state.rs`; strict checks and IR tests pass.
- [X] ACTIVITY 8.0.1.12 — Extract IR statement lowering into `ir/builder/statements.rs` (208 lines); preserve lowering behavior and strict checks.
- [X] ACTIVITY 8.0.1.13 — Extract IR expression lowering into `ir/builder/expressions.rs` (394 lines) and control-flow lowering into `ir/builder/control_flow.rs` (337 lines); reduce `builder.rs` below 520 lines.
- [X] ACTIVITY 8.0.1.14 — Extract IR builder state into nested `ir/builder/builder_state.rs`; maintain crate-visible methods and pass IR tests.
- [X] ACTIVITY 8.0.1.15 — Move networking unit fixtures into `net/tests.rs`; keep the production networking module below 520 lines and preserve IPv4/IPv6, TCP, UDP, CIDR, and resolver checks.
- [X] ACTIVITY 8.0.1.16 — Move HTTP adapter fixtures into `http/tests.rs`; keep the production HTTP adapter below 520 lines and preserve protocol, routing, TLS, callback, and bounds checks.
- [X] ACTIVITY 8.0.1.17 — Move LSP fixtures into `lsp/tests.rs`; keep the production LSP service below 520 lines while preserving navigation and completion checks.
- [X] ACTIVITY 8.0.1.18 — Move DAP fixtures into `dap/tests.rs`; keep the production DAP service below 520 lines while preserving framing, launch, breakpoints, and execution-session checks.
- [X] ACTIVITY 8.0.1.19 — Move BNWeb unit fixtures into `web/tests.rs`; preserve route, request/response, lifecycle, proxy, and bounds coverage while continuing production-module decomposition.
- [X] ACTIVITY 8.0.1.20 — Split runtime executor responsibilities into phase-local `runtime/executor/part*.rs`, `terminal.rs`, and `helpers.rs`; preserve the runtime facade and strict checks. Large provider dispatch methods remain scheduled for follow-up extraction.
- [X] ACTIVITY 8.0.1.21 — Isolate runtime host/support utilities and runtime tests; keep `runtime_impl.rs` below 520 lines with the same debug and clock behavior.
- [X] ACTIVITY 8.0.1.22 — Extract DataFrame join/append/select/add/count operations into focused executor methods (`part12.rs`/`part13.rs`); `part9.rs` is now below 520 lines with behavior and strict checks preserved.
- [X] ACTIVITY 8.0.1.23 — Extract `HOST.Net` dispatch from the general host dispatcher into `executor/part14.rs`; `part7.rs` is now below 520 lines and strict checks remain green.
- [X] ACTIVITY 8.0.1.24 — Extract address/endpoint/CIDR/reachability operations into `executor/part15.rs`; the remaining `part14.rs` scope is limited to TCP/UDP and collection transports.
- [X] ACTIVITY 8.0.1.25 — Extract TCP listener/stream and resolver branches into `executor/part16.rs`; `part14.rs` now contains only UDP and address-collection transport operations and is below 520 lines.
- [X] ACTIVITY 8.0.1.26 — Extract BNWeb state/request/response dispatch branches into `executor/part17.rs`, `part18.rs`, and `part19.rs`; `part4.rs` is now below 520 lines with strict checks preserved.
- [X] ACTIVITY 8.0.1.27 — Extract CLI frontend parsing/loading into `cli_frontend.rs` and move BNWeb routing/server lifecycle into `web/routing.rs` and `web/server.rs`; focused library tests and formatting pass.
- [X] ACTIVITY 8.0.1.28 — Extract parser expression precedence/primary parsing into `parser/expressions.rs` (472 lines); parser API and library tests remain green.
- [X] ACTIVITY 8.0.1.29 — Split the remaining parser declaration, statement, and token utility methods into four phase-local modules; all parser phase files remain below 520 lines.
- [X] ACTIVITY 8.0.1.30 — Extract IR lowering and helper functions into `ir/lowering.rs`, `ir/lowering_callable.rs`, and `ir/helpers.rs`; focused tests and strict Clippy pass.
- [X] ACTIVITY 8.0.1.31 — Split LLVM analysis, emission, tail dispatch, and helper logic into focused modules; all LLVM Rust files are below 520 lines and strict checks pass.
- [X] ACTIVITY 8.0.1.32 — Split semantic module analysis into `semantic/module_analysis.rs`, preserving module graph diagnostics and focused semantic behavior.
- [X] ACTIVITY 8.0.1.33 — Split semantic analyzer phases into eight phase-local modules and isolate HOST standard members in `semantic/host_defaults.rs`; preserve analyzer state and diagnostics.
- [X] ACTIVITY 8.0.1.34 — Split semantic type helpers and operations into `semantic/helpers*.rs`, `semantic/type_ops.rs`, and `semantic/type_names.rs`; retain public size/type APIs and strict visibility.
- [X] ACTIVITY 8.0.2 — Split semantic analysis, IR lowering/validation, parser phases, and LLVM lowering into phase-local modules with narrow state dependencies.
- [X] ACTIVITY 8.0.3 — Split CLI, web, HTTP, LSP, DAP, and networking files that exceed 520 lines; remove dead code and close strict Clippy warnings.
- [X] GATE G5 — Accept the refactoring only when no Rust file exceeds 520 lines and behavior remains unchanged under focused and full repository checks. Evidence: largest Rust file is 517 lines (`src/runtime_impl.rs`); `cargo test` (all suites), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` pass.

Directives: preserve `Source -> Tokens -> AST -> Semantic Analysis -> IR`;
`lib.rs` remains a re-export gateway; no new dependency or `unsafe`; use `?`
for propagation; allocation/index/conversion boundaries remain checked; extract
one testable slice at a time and keep each new file below 520 lines.

Exit evidence: `wc -l src/**/*.rs src/*.rs`; parser, semantic, IR, runtime,
codegen, CLI, module and protocol tests; `cargo fmt --check`; `cargo test`;
`cargo clippy --all-targets -- -D warnings`; `git diff --check`.

## SECTION 10 — Bounded dispatch and deferred web scope

Backport only the accepted 0.4 dispatch foundation without moving `BNDispatch`
into the language core. The concurrent `BNWeb` scope remains in the 0.4 plan.

### SPRINT 13 — Dispatch API and bounded foundation

- [X] ACTIVITY 8.1 — Freeze the `BNDispatch` execution, ownership, lifecycle, and resource contract. D-028/D-029 define bounded workers/tickets, pending-only cancellation, deadlines, isolated task ownership, and synchronized output forwarding.
- [X] DECISION D-028 — Accept opaque `Ticket` lifecycle, task-error propagation, cancellation, and close rules.
- [X] ACTIVITY 8.2 — Implement `HOST.NumProcs()` as the available logical-processor query for bounded worker-pool selection. (Semantic, IR, runtime, normative language/keyword/book documentation, and runtime test are complete.)
- [X] ACTIVITY 8.3 — Implement bounded `BNDispatch` queues, tickets/join, groups, barriers, semaphores, and mutexes over the accepted worker model. Native queues now run a fixed 1..64 worker pool, retain bounded pending tickets, execute named functions in isolated module copies, forward output through ticket ownership, and expose deterministic synchronization/timeouts.
- [X] GATE G5 — Accept the `BNDispatch` public API and execution model before concurrent BN work begins. Evidence: 87 library tests, 153 runtime tests, 44 CLI tests, 3 codegen tests, 7 IR tests, 46 semantic tests, strict Clippy, formatting, diff checks, and authorized local TCP/UDP socket validation all pass.

Exit evidence: fixed external-module signatures and limits; `HOST.NumProcs()`;
local queue/join/barrier/timeout/close tests; no unsafe shared interpreter state.

### SPRINT 14 — Deferred concurrent `BNWeb` scope

- [X] DECISION D-030 — Defer transport-to-BN callbacks, transport access logging, and HTTPS client/trust-root work to the 0.4 `BNWeb` revision.

Exit evidence: [`0.4-forward-plan.md`](0.4-forward-plan.md) owns the deferred
scope. No concurrent `BNWeb`, transport access-log, or HTTPS-client support is
claimed by 0.3.

## SECTION 11 — Vector and LLVM contract alignment

Apply the accepted declaration-time vector and typed LLVM contract after the
active refactoring and `BNDispatch` sprint sequence.

### SPRINT 15 — Contract alignment and lowering evidence

- [X] ACTIVITY 10.1 — Align local declaration-time vector dimensions, typed LLVM mappings, explicit CFG lowering, and resilient IR test construction with the accepted 0.3 contract.
- [X] GATE G7 — Accept the vector/LLVM contract only with local parser, semantic, runtime, IR, codegen, metadata, and repository evidence. Focused parser/semantic/runtime/IR/codegen tests, `cargo metadata --no-deps`, full `cargo test`, `cargo fmt --check`, strict Clippy, and `git diff --check` pass; unsupported lowering remains explicit as `BUILD_LOWERING_UNAVAILABLE`.

Exit evidence: local declaration-time vector dimensions remain fixed after
binding; signatures and fields stay literal-only; LLVM lowering emits typed
blocks/terminators without executing BN programs; unsupported lowering and
overflow behavior are recorded with executable checks.
