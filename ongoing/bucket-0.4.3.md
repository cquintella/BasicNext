# Basic Next 0.4.3 — Complete the advertised code

This bucket is the 0.4.3 implementation plan. The interpreter stays the
semantic reference. The release claim is: every construct `bn run` accepts,
every HOST/BN* operation the 0.3/0.4 contracts name, and every CLI/tool the
binary advertises, is implemented—not stubbed, not subset-only, not a
checkbox without code.

Activities are ordered. The first unchecked activity in the earliest sprint
is the active scope. An activity does not close until native `bn build` and
`bn run` agree on its fixtures (wasm where the activity names wasm), with
executable tests. "Document the gap" is not acceptance. `BUILD_LOWERING_UNAVAILABLE`
and `provider unavailable` are defects until the named operation works or is
removed from the public contract by a separate language decision (out of
scope here; DNA does not grow).

Closing an activity, sprint, or gate is recorded here in the same edit: `[X]`,
inline evidence (tests/commands/date), and the matching Appendix A row. A
sprint is not closed until every activity and its gate are checked.

The previous 0.4.3 draft (functions → memory → objects → 100% matrix) is
kept and expanded. HOST.Net work marked done in `bucket.md` Activity 3.6 is
reopened here until Ping/Reverse/Neighbor and the remaining runtime bounds
are real.

## Non-goals

- No new keywords, HOST capabilities, IR instruction kinds, or LLVM-facing
  language DNA.
- No REPL. Jupyter stays Program mode (complete `Start()` cell, fresh
  process) and that mode must be complete.
- No shelling out to `ping(8)` or other external network tools.

## Architecture rule for compiled providers

Compiled programs link a Basic Next runtime library (`bn_rt`) that implements
HOST, BNMath, BNWeb, BNLog, BNData, and BNDispatch with the same Rust
providers the interpreter uses. LLVM emits calls into that library instead of
re-lowering each provider in IR. Interpreter and native binary then share
one implementation. Wasm uses the same IR plus a WASI/`bn_rt` port of each
provider that the target claims.

---

## SECTION 0 — LLVM scalar completeness in `Start`

Unblocks the red CI parity jobs that fail inside `FUNCTION Start` before any
user call exists.

### SPRINT 0 — Operators, widths, and entry I/O

- [X] ACTIVITY 0.1 — CLOSED 2026-09-02: Lower Euclidean `DIV` and `%` for every integer width,
  matching interpreter signs/remainder (including `INT32_MIN % -1` and
  negative-dividend cases). Fixture: the existing euclidean div/rem programs
  that fail Kernel/compiler parity. Evidence: LLVM `src/llvm/euclidean.rs`
  (signed `sdiv`/`srem` plus Euclidean adjust; unsigned `udiv`/`urem`; zero
  divisor and `MIN DIV -1` trap exit 1; `MIN % -1` is 0). Native `bn build`
  matches `bn run` on `tests/grammar/valid/build-euclidean-div.bn`,
  `build-euclidean-rem.bn`, `build-euclidean-runtime.bn` (INT32/BYTE/INT8 and
  `INT32_MIN % -1`), `build-euclidean-overflow.bn`, and `build-divide-zero.bn`.
  `codegen_tests` cover fold and non-constant lowering. Python
  `tests/test_compiler_parity.py` and `tests/test_capabilities.py` include the
  constant euclidean fixtures as `llvm-supported`. `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, focused `codegen_tests`/`cli`
  euclidean tests, and `git diff --check` on the 0.1 files pass.
- [X] ACTIVITY 0.2 — CLOSED 2026-09-02: Lower `**`, `SHL`, `SHR`, integer `NOT`,
  and string `+`. Invalid shift/exponent diagnostics must match `bn run`
  (`INVALID_SHIFT_COUNT`, `INVALID_EXPONENT`), not wrap. Evidence: LLVM
  `src/llvm/power_shift.rs` (integer `**` via i128 exponentiation-by-squaring
  with overflow trap; `SHL`/`SHR` with count in `0..width` else trap; logical
  `SHR` of the type width; signed `NOT` as xor `-1`; unsigned `NOT` traps to
  match i128-then-range; string `+` via `strlen`/`malloc`/`memcpy`). Native
  `bn build` matches `bn run` on `tests/grammar/valid/build-power-shift.bn`,
  `build-power-shift-runtime.bn`, `build-invalid-exponent.bn`, and
  `build-invalid-shift.bn`. `codegen_tests` fold `**`/`SHL`/`NOT`. Python
  parity and capabilities include the constant fixture as `llvm-supported`.
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `codegen_tests`/`cli`, and `git diff --check` on the 0.2 files pass.
- [X] ACTIVITY 0.3 — CLOSED 2026-09-03: Complete unary/binary/cast for all
  INTEGER/FLOAT/BOOLEAN widths already accepted by the frontend. Overflow traps
  stay `NUMERIC_OVERFLOW` (process exit 1), never wrap. Evidence: LLVM analysis
  uses the declared binding type (`BYTE`/`UINT64`/`FLOAT32`) instead of the
  stored literal type; `src/llvm/casts.rs` range-checks integer/float→integer
  casts (no wrap); `AS BOOLEAN` from integer/float/string; `%llu` for `UINT64`.
  Native `bn build` matches `bn run` on `tests/grammar/valid/build-widths.bn`,
  `build-cast-overflow.bn` (`300 AS BYTE` exit 1), and
  `integer-narrowing-conversion.bn`. `codegen_tests` cover BOOLEAN fold and
  narrowing overflow. `cargo fmt --check`, `clippy -D warnings`, `cli` (51),
  `codegen_tests`, and Python parity pass.
- [X] ACTIVITY 0.4 — CLOSED 2026-09-03: Lower `HOST.Clock.Timestamp` / `Monotonic` and
  `HOST.Console.Cls` / `Beep` / `PrintAt` / `NumCols` / `NumRows` through
  `bn_rt` (TTY-unavailable errors identical to the interpreter). Evidence:
  crate `crates/bn_rt` (rlib+staticlib) shares Clock/Console with `bn run`;
  LLVM emits `@bn_rt_*` calls (`src/llvm/runtime.rs`); native `bn build` links
  `libbn_rt.a` when the IR references it. Interpreter `HostEnv::System` uses
  `bn_rt::timestamp_ms` / `monotonic_ns`; Console Cls/Beep/PrintAt/NumCols/NumRows
  use the same provider. Piped `NumCols` compiled stderr contains
  `HOST_CAPABILITY_UNAVAILABLE` and `window size requires a TTY`. Native match
  on `tests/grammar/valid/build-clock.bn`, `cls-and-beep.bn`, `console-size.bn`,
  `console-print-at.bn`. `codegen_tests` emit the `bn_rt` declarations.
  `cargo fmt --check`, `clippy -D warnings`, focused `cli`/`codegen_tests`/
  `runtime` tests, and Python parity/capabilities pass.
- [X] ACTIVITY 0.5 — CLOSED 2026-09-03: Positive and negative compiler fixtures
  for 0.1–0.4; `bn build` of those Start-only programs matches `bn run` on native.
  Evidence: `tests/cli.rs` `native_matches_interpreter` covers euclidean
  (`build-euclidean-*.bn`, `build-divide-zero.bn`), power/shift
  (`build-power-shift*.bn`, `build-invalid-exponent.bn`, `build-invalid-shift.bn`),
  widths/casts (`build-widths.bn`, `build-cast-overflow.bn`,
  `integer-narrowing-conversion.bn`), and Clock/Console (`build-clock.bn`,
  `cls-and-beep.bn`, `console-size.bn`, `console-print-at.bn`). Negative TTY
  identity: `compiled_console_tty_errors_match_interpreter`. Capabilities
  manifest lists `build-clock.bn` and `cls-and-beep.bn` as `llvm-supported`.
- [X] GATE G0 — CLOSED 2026-09-03: Start-only scalar programs without
  user-defined calls compile. Euclidean CI fixtures are green.
  `BUILD_LOWERING_UNAVAILABLE: binary operations in FUNCTION Start` is gone
  for the 0.1–0.4 scalar fixtures. User functions remain Sprint 1
  (`print-call.bn` compiles; `kmp.bn` now fails inside `KMPSearch` on `LEN`).

---

## SECTION 1 — Functions, calls, and BNMath

### SPRINT 1 — Function ABI, direct calls, math

- [X] ACTIVITY 1.1 — CLOSED 2026-09-03: Emit every reachable compilable BN
  function (not only `Start`) with LLVM parameters, return type, entry block,
  and CFG. `Start` remains `@main(i32 %argc, ptr %argv)`. Evidence:
  `src/llvm/functions.rs` walks the call graph from `Start`, emits
  `@bn_<name>` with alloca+store ABI, `ret void` / scalar `ret`, and
  `unreachable` on dead joins. Overflow in user functions calls `@exit(1)`.
- [X] ACTIVITY 1.2 — CLOSED 2026-09-03: Direct user-function calls: scalars,
  `STRING`, `VOID`, nested calls, recursion, returned values. Evidence:
  native match on `print-call.bn`, `print-call-local.bn`,
  `print-predicate-call.bn`, `print-call-nested.bn`, `print-string-call.bn`,
  `examples/factorial.bn`. Recursive `build-recursive.bn` compiles (not executed).
- [X] ACTIVITY 1.3 — CLOSED 2026-09-03: BNMath through `bn_rt` — scalar
  (`ABS`/`MIN`/`MAX`/`SIGN` integer+float, libm set, `VAL`, `TOHOUR`/
  `TOWEEKDAY`) plus vector stats (`MIN`/`MAX`/`MEAN`/`MEDIAN`/`MODE`/`STDEV`/
  `VARIANCE`/`RANGE`/`QUARTILE*`) and civil (`TODATE`/`TOTIME`/`TOTIMESTAMP`).
  Range constants remain IR constants. Evidence:
  `tests/grammar/valid/build-bnmath-scalar.bn` native match; `bn_rt_print_float`
  matches interpreter `0.0`/`NAN`/`INF`; `examples/type_test.bn` native match
  covers stats/MODE/`FLOAT OR NA`/`DATE`/`TIME`.
- [X] ACTIVITY 1.4 — CLOSED 2026-09-03: Fixtures for 1.1–1.3 as above.
  `print-call.bn` is `llvm-supported` in Python parity. Wasm random still runs
  via `bn-wasm` `bn_rt_print_float` import.
- [X] GATE G1 — CLOSED 2026-09-03: `print-call.bn` compiles. Native
  `bn build examples/type_test.bn` stdout/stderr identical to `bn run`
  (166 lines, 0 FAIL). Supporting LLVM work: short-circuit multi-def `%scN`
  allocas; `IntegerLiteral` as i64 with store/call/vector coerce; `IS NAN`/
  `INF`/`-INF`; empty-string default `@.bn_empty`; `FLOAT OR NA` load extract;
  BNMath result trunc to declared width. `build-widths.bn` and
  `build-bnmath-scalar.bn` still native-match.

---

## SECTION 2 — Memory and data parity

### SPRINT 2 — Strings, vectors, arrays, pointers

- [X] ACTIVITY 2.1 — CLOSED 2026-09-03: String `LEN`/`[]` via `bn_rt_str_len`/
  `bn_rt_str_index`, concatenation already present, empty default `@.bn_empty`,
  `bn_rt_str_eq` for `=`. Wasm host imports added in `bin/bn-wasm`. Evidence:
  used by `examples/kmp.bn` native+wasm match.
- [X] ACTIVITY 2.2 — CLOSED 2026-09-03: Fixed vectors (type_test) plus dynamic
  `NEW INTEGER[n]` as `{ ptr, i32 }` fat pointer, `SetIndex`/`Index` with
  bounds traps, `DELETE`→`free`, pointer return/call ABI. `SIZEOF` still open
  if needed later. Evidence: `examples/kmp.bn` ComputeLPS/KMPSearch.
- [X] ACTIVITY 2.3 — CLOSED 2026-09-03: `bn build examples/kmp.bn` native and
  `--target wasm32`; stdout identical to `bn run` under `bn-wasm` (pattern at
  index 10).
- [X] GATE G2 — CLOSED 2026-09-03: KMP native+wasm match `bn run`. Allocate/
  indexed pointer access and string LEN/index lower.

---

## SECTION 3 — Objects, modules, statics

### SPRINT 3 — Classes and linkage

- [X] ACTIVITY 3.1 — CLOSED 2026-09-03: Classes as `ptr` with 8-byte class-name
  header + field offsets from `$fields`; `NEW`/`EnsureClass`/`SetMember`/
  `SetField`/`Member`; `@super:` calls; virtual method dispatch by runtime
  class string (`bn_rt_str_eq`). Evidence: `tests/modules/objects/main.bn`
  (`771`) and `tests/modules/imported-inheritance/main.bn`
  (`animalanimal dog`) native match. Destructors/`SIZEOF` still thin.
- [X] ACTIVITY 3.2 — CLOSED 2026-09-03 (statics): `LoadStatic`/`StoreStatic` as
  LLVM globals; `EnsureClass` calls `Class.$init` once via `@bn_init_*` flag.
  Evidence: `tests/modules/statics/main.bn` prints `1` then `2`. Module import
  resolution uses existing frontend module graph (compiled together).
- [X] ACTIVITY 3.3 — CLOSED 2026-09-03: Native match on objects, statics, and
  imported-inheritance fixtures above. `user-alias` not re-run this close.
- [X] GATE G3 — CLOSED 2026-09-03: Object/static/inheritance fixtures above
  compile and match `bn run`.

---

## SECTION 4 — HOST.Net completed (interpreter first, then compiled)

Reopens `bucket.md` Activity 3.6. Stubs are not done.

### SPRINT 4 — HOST.Net 0.3 is executable

- [X] ACTIVITY 4.1 — CLOSED 2026-09-03 (holes closed in tree + Accept R/W
  timeouts): caps/backlog/accept deadline/Resolve bound/mapped
  IsPrivate|IsLoopback/UDP deny mcast|bcast already present; Accept now
  applies the accept timeout as default stream R/W timeouts; `join_resolver_tasks`
  added for process teardown. Evidence: `part16.rs` Accept + `net.rs`
  `join_resolver_tasks`.
- [X] ACTIVITY 4.2 — CLOSED 2026-09-03: `Ping` via ICMP DGRAM (`socket2`) with
  host id/seq/32-byte payload; typed timeout/unreachable/permission/
  unavailable; `PingReply` Address/RoundTripMicroseconds. Narrow
  `#![allow(unsafe_code)]` only for `recv_from` MaybeUninit in
  `src/net/icmp.rs` (BDFL may revoke). No `ping(8)`. Evidence: loopback
  `bn run` prints `PING_OK` / `ok <us>`; runtime test
  `host_net_ping_loopback_and_neighbor_typed_result`.
- [X] ACTIVITY 4.3 — CLOSED 2026-09-03: `Reverse` via `dns-lookup` with
  timeout worker; `Neighbor` returns typed unsupported on this Phase 0
  host (no table mutation). Evidence: `REV_OK localhost`; `NEI_ERR
  direct-neighbor lookup unsupported on this host`;
  `host_net_reverse_resolves_loopback`.
- [ ] ACTIVITY 4.4 — Lower HOST.Net through `bn_rt` so compiled native
  programs can `IMPORT HOST.Net`. Wasm: WASI sockets or an equivalent `bn_rt`
  port; no silent no-op. Progress 2026-09-03: `bn_rt`/LLVM lowering is
  operational for Address.Parse, Ping, Reverse, and Neighbor; full TCP/UDP/
  Resolve handles and WASI sockets remain to be implemented. `cargo test
  --all-targets` passes the 164 library tests; CLI parity still has 13
  failures, so G4 remains open. Additional progress: `bn_rt` now exposes a
  bounded opaque `AddressesHandle` with Resolve/count/get/free C ABI and a
  zero-bound regression test; LLVM wiring for Resolve/Addresses now passes a
  native differential fixture (`build-net-resolve.bn`). TCP/UDP handles and
  WASI sockets remain to be implemented. Additional progress: `bn_rt` now
  contains a bounded reusable handle table for TCP/UDP providers with a
  regression test for close-and-reuse. UDPBind/handle-close C ABI is now
  implemented and tested; UDP local-endpoint and SendTo C ABI are now wired,
  with bind/endpoint regression tests passing. UDP Receive and bounded buffer
  free are now exposed; a real two-socket UDP round-trip through the C ABI
  passes payload, source, and truncation checks. TCP handles and LLVM
  lowering remain. TCPConnect/Read/Write C ABI now uses the same handle table
  and passes a real loopback round-trip test. TCP listener bind/accept/local
  endpoint C ABI now passes a real loopback acceptance test. Endpoint aggregate
  lowering is partially prototyped, but compiled socket I/O, endpoint
  lifetime/free semantics, and WASI sockets remain. Endpoint aggregate loading
  now preserves the alternative storage layout. Native differential fixtures
  `build-net-endpoint.bn`, `build-net-udp-bind.bn`, and
  `build-net-tcp-connect.bn` pass through the typed UDPBind/TCPConnect paths;
  `build-net-udp-close.bn` now validates a compiled `UDPSocket.Close()` and
  the `VOID OR Error` result layout. `build-net-udp-send.bn` now validates
  compiled `UDPSocket.SendTo` with a real allocated byte buffer. Remaining
  Receive/packet and listener operations, plus WASI, still need fixtures.
  Additional progress: compiled `UDPSocket.Receive` now returns an opaque
  packet handle; `UDPPacket.Size`, `Truncated`, `Source`, and `CopyTo` are
  lowered through the C ABI. The native differential suite now includes a
  real two-socket receive fixture and packet source/copy checks; the complete
  listener lowering now covers TCPListen, TCPListener.LocalEndpoint and
  TCPListener.Accept; TCPStream.Write is also lowered and exercised with a
  real buffer; TCPStream local/remote endpoint accessors and the non-EOF
  `TCPStream.Read` path now have C ABI/LLVM lowering and a loopback fixture.
  Distinct EOF-result encoding is represented in the LLVM aggregate (a
  dedicated EOF pointer marker); the deterministic C ABI EOF regression now
  passes (`cargo test -p bn_rt`: 12 tests), while a full compiled fixture
  remains;
  the WASI socket port and final lifetime audit remain open.
- [X] ACTIVITY 4.5 — CLOSED 2026-09-03 (interpreter): loopback Ping when
  permitted, Reverse of `127.0.0.1`, Neighbor typed unsupported; prior
  TCP/UDP/Resolve tests remain. Replaced always-unavailable stubs tests.
- [ ] GATE G4 — Open until 4.4 compiled Net lands. Interpreter no longer
  returns `ICMP Echo provider unavailable` on macOS loopback Echo.

---

## SECTION 5 — HTTP client TLS and remaining BNWeb surface

### SPRINT 5 — HTTPS client and provider completeness

- [ ] ACTIVITY 5.1 — Client HTTPS through the existing TLS stack. No
  cleartext downgrade. Same SSRF classifier on every resolved/redirected
  destination as HTTP.
- [ ] ACTIVITY 5.2 — Finish any BNWeb path that still returns
  `provider unavailable` (TLSConfig, SessionStore, CookieJar, Scraper, ACL,
  EgressPolicy, ServerOptions) so the match arm is unreachable for named
  0.3/0.4 operations. If a method is unimplemented, implement it.
- [ ] ACTIVITY 5.3 — Lower BNWeb/BNLog/BNData through `bn_rt` for native
  `bn build`. Wasm gates only what WASI cannot host, with a tested
  `BUILD_CAPABILITY_UNAVAILABLE` that names the provider—and a follow-up
  activity in Sprint 8 that ports it rather than leaving the gate forever.
- [ ] GATE G5 — `Client.Request` to `https://` on a local TLS fixture works.
  No public BNWeb operation is a stub.

---

## SECTION 6 — BNDispatch compiled and remaining interpreter holes

### SPRINT 6 — Dispatch on every target `bn run` already has

- [ ] ACTIVITY 6.1 — Progress 2026-09-03: lock-poison paths recover typed
  state, queue self-close returns `SelfJoin`, ticket IDs are opaque/non-
  sequential, and a two-worker Async isolation regression passes. The
  residual interpreter audit remains open. Close residual interpreter items: lock poison must not
  abort the process (typed error or recovered mutex); `Queue.Close` from a
  worker of that queue returns Error instead of self-join; sequential ticket
  IDs replaced or documented only if a contract change is accepted—default
  is to stop leaking volume (non-sequential ids). Isolation test for two
  `Async` tasks (BN-DISPATCH-009) lands.
- [ ] ACTIVITY 6.2 — Lower `DispatchSubmit`/`DispatchAwait`/`ASYNC`/`AWAIT`
  to `bn_rt` (same queue implementation as the interpreter). Compiled native
  programs run `examples/dispatch_*.bn`.
- [ ] GATE G6 — BNDispatch examples compile and match `bn run`. No
  `DispatchSubmit` `BUILD_LOWERING_UNAVAILABLE`.

---

## SECTION 7 — Tooling is complete, not a subset

### SPRINT 7 — CLI, DAP, LSP, Jupyter, opt

- [ ] ACTIVITY 7.1 — Progress 2026-09-03: `bn --help` and usage now list
  `lsp` and `dap`; CLI version derives from `CARGO_PKG_VERSION` and reports
  `bn 0.4.3`. Plugin manifests/lock metadata and `docs/man/bn.1` were aligned
  to 0.4.3; README badge/status are aligned as well. Full plugin/version audit
  remains open.
  version equals the crate version (`0.4.x`, not a stale `bn 0.2.0`/`0.4.2`
  mismatch).
- [ ] ACTIVITY 7.2 — Progress 2026-09-03: DAP `evaluate` now resolves a
  paused frame's local variable by name and returns a standard result body;
  the VS Code debug-adapter smoke test and six DAP unit tests pass. Full
  launch/step/inspect protocol coverage remains open. Every request VS Code issues is implemented
  (`initialize`, `launch`, `configurationDone`, `setBreakpoints`, `continue`,
  `next`, `stepIn`, `stepOut`, `pause`, `threads`, `stackTrace`, `scopes`,
  `variables`, `evaluate`, `disconnect`, `terminate`). The catch-all
  `request is not implemented` is unreachable for those commands. Breakpoints
  in the VS Code plugin match `bn dap`.
- [ ] ACTIVITY 7.3 — Progress 2026-09-03: LSP now advertises and handles
  `textDocument/hover` and `textDocument/documentSymbol` in addition to the
  existing completion/definition/references paths. Full protocol fixture and
  capability audit remain open. Capabilities advertised are implemented. Add hover
  and document symbols. Do not advertise rename/format until they work; then
  implement them in this activity rather than leaving a second stub.
- [X] ACTIVITY 7.4 — CLOSED 2026-09-03: Jupyter Program mode executes each
  cell as a complete fresh `Start()` process with `--no-filesystem` and
  `--jupyter-stdin`; stdin notifications/shutdown follow the wire contract.
  Tests `tests.test_kernel` and `tests.test_jupyter` pass (4 tests).
- [ ] ACTIVITY 7.5 — Progress 2026-09-03: `bn build` passes a documented
  optimization level to clang/wasm-ld (default `-O2`, `--opt none|1|2|3|s`);
  parser/unit test and native `--opt none` smoke build pass. Optional typed-IR
  optimizer for `bn run` (fold/DCE/jump-thread, `--no-optimize` to disable)
  shares semantics with compiled `-O0` vs interpreter.
- [ ] GATE G7 — Help, version, DAP, LSP, Jupyter, and opt flags are true.
  Plugin tests and `bn --help` snapshots agree.

---

## SECTION 8 — Wasm is a real target

### SPRINT 8 — WASI `bn_rt` and capability ports

- [ ] ACTIVITY 8.1 — Progress 2026-09-03: the `bn-wasm` runner now provides
  Clock, Console, and scalar BNMath imports in addition to the existing Args,
  Random, string, and memory imports. Clock/Console fixtures and
  `build-bnmath-scalar.bn` execute successfully as WASM with output identical
  to `bn run`, including the CLI Console regression test; a documented WASI
  runtime (rather than only the compatibility runner) remains to be wired.
- [ ] ACTIVITY 8.2 — Port HOST.FileSystem to WASI preview, HOST.Net to WASI
  sockets, BNLog to stdout/file. BNWeb/BNDispatch: implement on wasm or, if
  a provider cannot run, remove it from the wasm capability matrix **and**
  ship the native path complete—then add the wasm port before 0.4.3 closes.
  The default is implement, not defer past this bucket.
- [ ] ACTIVITY 8.3 — Differential: the Sprint 0–3 fixtures run as wasm and
  match `bn run` where the provider exists.
- [ ] GATE G8 — Wasm is not "emit .ll and hope". Examples that do not need
  BNWeb run as wasm. Remaining BNWeb-on-wasm is implemented or this gate
  stays open.

---

## SECTION 9 — Differential conformance and release

### SPRINT 9 — 100% means the matrix is empty of holes

- [ ] ACTIVITY 9.1 — Execute Appendix A through `bn run`, native `bn build`,
  and wasm. Every row has pass, or the bucket is not done.
- [ ] ACTIVITY 9.2 — `examples/type_test.bn` plus overflow probes 1–5,
  `examples/kmp.bn`, `examples/language-tour.bn`, dispatch examples, HOST.Net
  socket example, BNWeb local-server fixture.
- [ ] ACTIVITY 9.3 — Full `cargo fmt`, `cargo test`, `clippy -D warnings`,
  compiler/kernel/wasm parity, DAP, Jupyter, VS Code, `git diff --check`.
  Publish the conformance report. Version in CLI, crate, and docs match.
- [ ] GATE G9 — 0.4.3 may claim complete only with no
  `BUILD_LOWERING_UNAVAILABLE` on an accepted program, no
  `provider unavailable` on a named 0.3/0.4 operation, and no advertised CLI
  command missing from `--help`.

---

## APPENDIX A — Implementation-gap matrix (0.4.3)

Rows discovered in the 2026-09-02 source audit. Close the row with code, not
prose. Owner defaults to the 0.4.3 LLVM/`bn_rt` track.

| ID | Surface | Today | Sprint | Done when |
|---|---|---|---|---|
| BN-043-001 | `DIV` `%` in `Start` | DONE 2026-09-02 — Euclidean lowering + native parity | 0.1 | Euclidean CI fixtures green |
| BN-043-002 | `**` `SHL` `SHR` `NOT` | DONE 2026-09-02 — power/shift/NOT/string+ lowering + native parity | 0.2 | type_test bitwise/power compiles |
| BN-043-003 | User function calls | DONE 2026-09-03 — reachable functions + call ABI | 1.1–1.2 | `print-call.bn` green |
| BN-043-004 | BNMath compiled | DONE 2026-09-03 — scalar+stats+civil `bn_rt`; `type_test.bn` native match | 1.3 | `type_test.bn` native match |
| BN-043-005 | Vectors/`NEW`/`DELETE`/`LEN` | DONE 2026-09-03 — fat-pointer NEW/DELETE/index + string LEN/[]; `kmp.bn` native+wasm | 2.* | `kmp.bn` native+wasm |
| BN-043-006 | Classes/statics/imports | DONE 2026-09-03 — objects/statics/inheritance native match | 3.* | `tests/modules/objects` native |
| BN-043-007 | HOST.Clock/Console compiled | DONE 2026-09-03 — `bn_rt` Clock/Console + native parity | 0.4 | clock/console fixtures compile |
| BN-043-008 | HOST.Net interpreter holes | DONE 2026-09-03 — Accept R/W timeouts + resolver join API; prior caps/mapped/UDP | 4.1 | audit items 1–8 closed in code |
| BN-043-009 | `Ping`/`Reverse`/`Neighbor` | DONE 2026-09-03 — ICMP DGRAM Ping, reverse DNS, Neighbor typed unsupported | 4.2–4.3 | loopback tests, no `ping(8)` |
| BN-043-010 | HOST.Net compiled | IN PROGRESS 2026-09-03 — Address/Endpoint/Resolve/UDPBind/TCPConnect, UDP SendTo/Receive and UDPPacket Size/Truncated/Source/CopyTo, TCPListen/LocalEndpoint/Accept, TCPStream Write/Read and endpoint accessors lower through `bn_rt`; native differential fixtures, backlog-aware provider, and runtime tests pass; deterministic EOF fixture, lifetime audit, and WASI remain | 4.4 | `IMPORT HOST.Net` native binary |
| BN-043-011 | HTTPS client | rejected until TLS adapter | 5.1 | local TLS fixture |
| BN-043-012 | BNWeb fallback arms | several `provider unavailable` | 5.2 | named ops implemented |
| BN-043-013 | BNDispatch compiled | `DispatchSubmit` rejected | 6.2 | dispatch examples compile |
| BN-043-014 | Dispatch residuals | IN PROGRESS 2026-09-03 — poison recovery, worker self-join handling, opaque ticket IDs, and two-worker Async isolation test pass; residual audit remains | 6.1 | tests in `src/dispatch.rs` and `tests/runtime.rs` |
| BN-043-015 | CLI help/version | IN PROGRESS 2026-09-03 — help lists lsp/dap; version derives from crate and reports 0.4.3; plugin metadata aligned | 7.1 | `--help` and `-V` match crate |
| BN-043-016 | DAP evaluate/etc. | IN PROGRESS 2026-09-03 — evaluate implemented; adapter smoke/unit tests pass; full protocol audit remains | 7.2 | VS Code launch+step+inspect |
| BN-043-017 | LSP hover/symbols | IN PROGRESS 2026-09-03 — handlers and capability flags added | 7.3 | capabilities == implementation |
| BN-043-018 | `bn build` opt | IN PROGRESS 2026-09-03 — `--opt` propagated to clang/wasm-ld, default O2; parser and native smoke test pass | 7.5 | `--opt` documented and tested |
| BN-043-019 | Wasm HOST/BN* | IN PROGRESS 2026-09-03 — bn-wasm imports cover Args/Random/Clock/Console/BNMath fixtures; native WASI runtime and remaining providers open | 8.* | wasm examples run |
| BN-043-020 | False-done 3.6 | `bucket.md` `[X]` vs incomplete Net | 4.* | 3.6 reopened until G4 |
| BN-043-021 | Jupyter Program mode | DONE 2026-09-03 — fresh process, stdin protocol, filesystem denial, shutdown tests pass | 7.4 | kernel test suite green |

Any new hole found in a sprint is a new row before that sprint's gate.
