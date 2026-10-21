# Known issues — Basic Next 0.6.5 working tree (open; updated 2026-10-16)

`issues.md` was retired on 2026-10-13 after migration here. Every still-open
finding it contained is already represented by an entry below: BNWeb native
(K14), BN functions of provider modules (K48, superseded by P.15), SQLite
rowid (K73), `HOST.FileSystem` `ReadLine` (K74), SQLite import diagnostics
and empty BLOB (K76, K78), DAP/LSP missing protocol commands, and the product
hygiene debts already tracked by the H section. The stale, resolved native HOST
and IR entries were not migrated.

Only open issues are listed. Resolved entries, with their cause, fix and
evidence, moved on 2026-10-11, 2026-10-14 and 2026-10-16 to `done/known_issues-0.6.5-resolved.md`
(K2, K3, K4–K9, K13, K18, K19, K23–K30, K33–K43, K47–K53, K56, K57, K59, K61,
K25, K67, K68, K69, K70, K71, K72, K75, K79, K80, K83, K84); the bucket still cites them by number.
K1, K22 and K85 were rewritten in place on 2026-10-16 (Sprint 7 stage 1) and stay open.

First measured (2026-10-09) on branch `bucket-0.6.5-corrections` plus the uncommitted 0.6.5c
work. Corpus probe: the 213 programs of `examples/` and
`tests/grammar/valid/`: `bni check` passes on all 213; `bnc` builds 203; 9
stop with a support diagnostic, 1 with a defect (K4).

Severity: **critical** (security or data integrity), **high** (wrong
behaviour), **gap** (declared native support gap: refused with a support
diagnostic, never a wrong result), **minor**. "Since" says whether the issue
predates this session (checked on a clean `HEAD` worktree where marked).

**Bar (Carlos, 2026-10-09):** 0.6.5 is a production release without errors.
Every open entry here, gaps included, is fixed before the `v0.6.5` tag;
interim fixes (K1, K3) do not count. Each entry names its 0.6.5 bucket.

---

## Critical

### K1 — A `STRING` holding NUL stops a native program (denial of service) — RESOLVED 2026-10-17

- **Resolved 2026-10-17 (Sprint 7 stage 1 completed; SP7.8, SP7.9, SP7.10).**
  The `TARGET_UNSUPPORTED_TYPE` stop is gone.
  `nul_in_a_string_matches_on_both_backends` (`tests/compiler_parity.rs`)
  matches `bni` on `char`, `ops`, `forged`, `forged_file`, `input`,
  `file_read_all`, `file_read_line`, `json`, `sqlite`, and `exec` (Unix and Windows).
  The value-matrix cell `STRING / holds U+0000` is active and passes.
  `PRINT_STR` declaration wired in `runtime_abi.rs`.
  String lengths are authoritative via header `len`.
  Verified across both Unix (macOS) and Windows (`bn-windows`).
- **History.** Before 0.6.5b S7.c (option C) the native value was cut at
  the NUL in silence: `"admin\0x"` became `"admin"`. Option C traded that
  for the stop. The 2026-10-14 header guessed a magic number and was
  rejected (bucket SP7.R). Stage 1 (G7-D1–D6, alternative A) puts a
  16-byte header before every STRING by construction. The LLVM type stays
  `ptr`. The SSA value `{ ptr, len }` is stage 2.
- **Since.** Truncation predates this session; the stop since `2c0b6ea`;
  the stop is gone on the fixtures above since the uncommitted stage-1
  tree of 2026-10-16.

---

## Native support gaps (refused with a support diagnostic)

### K10 — Store into a `STRUCT` local declared without an initializer — RESOLVED 2026-10-14 (Sprint 1 / V.S3)

- **Resolved 2026-10-14 (Sprint 1 / V.S3):** Value witnesses (`@__bn_copy_*` and `@__bn_destroy_*`) synthesize deep copy and destruction for structs holding vectors and nested structs. `Instruction::Store` and indexed stores perform deep copy with drop glue on existing slots. Verified in `tests/value_matrix.rs` (41/41 cells clean) and `tests/compiler_parity.rs` (`indexed_struct_and_static_fields_preserve_language_semantics`).


### K11 — `IS POINTER TO T` on an alternative — RESOLVED 2026-10-17 (P.12 / T.K11)

- **Resolved 2026-10-17 (P.12 / T.K11):** `NULL` arm and region pointer arm in alternatives verified on both backends via `tests/parity_regressions.rs` (`null_named_pointer_alternative_is_not_the_pointer`, `qualified_null_pointer_alternative_is_not_the_pointer`, `nullable_integer_region_is_reads_length`). Single-value `POINTER TO Box` cannot be instantiated via `NEW` in Basic Next 0.6.

### K12 — Local vector whose dimension is an expression — RESOLVED 2026-10-17 (P.12 / T.K12)

- **Resolved 2026-10-17 (P.12 / T.K12):** `Instruction::Default` with dynamic 1D dimension lowers via counted stack allocation (`alloca T, i64 %len`) and `llvm.memset.p0.i64` zero-initialization, checking `< 0` and trapping with `INVALID_VECTOR_DIMENSION` (`crates/bn_llvm/src/llvm/emission1.rs`). Verified via `tests/parity_regressions.rs` (`dynamic_vector_dimension_allocates_and_zeros_memory`).

### K14 — `BNWeb` under `bnc` — RESOLVED 2026-10-17 (P.13 / E.S1–E.S6)

- **Resolved 2026-10-17 (P.13 / Section E):** Isolated `BNWeb` into `libraries/bn_web` (`libbn_web.a`) with zero coupling in `bn_rt` and `bnc`, lowered all `BNWeb` methods in `crates/bn_llvm/src/llvm/web_ops.rs`, and verified full parity across all S6 fixtures via `tests/parity.rs` (`bnweb_s6_parity_fixtures_pass_both_interpreter_and_native`).

### K15 — Wasm runs only a subset — RESOLVED 2026-10-20 (target withdrawn)

- **Resolved 2026-10-20 (BDFL).** `bnc` no longer emits WebAssembly.
  `--target wasm32` is an invalid option. `bin/bn-wasm` and
  `tests/wasm_parity.rs` are removed.

---

## Divergences and minor issues

### K16 — `BN_ARC_TRACE` labels a region differently — RESOLVED 2026-10-17

- **Resolved 2026-10-17:** Emitted code passes the element type symbol (`.bn_cls_<Element>`)
  to `bn_rt_arc_register`, aligning native region traces with interpreter output (`Float64#0`, `Int32#0`, etc.).
  Verified by `tests/parity_regressions.rs::arc_trace_region_labels_match_on_both_backends`.

### K17 — `CopyIntegerColumn` range check differs

- **Planned:** `ongoing/bucket-0.6.5.md` C.S8.c.
- **Resolved 2026-10-16:** Unified on `copy_integer_element` in DataFrame core
  (`crates/bn_rt/src/dataframe.rs`). Both `bn_rt_dataframe_copy_integer` and
  `bn_lib_data`'s adapter call the same rule, returning `INVALID_FORMAT`
  ("integer out of range") for cells beyond `INT32`. Verified with 64-bit
  overflow tests on both backends (`dataframe_abi::tests::copy_integer_rejects_64bit_overflow_with_invalid_format`,
  `tests::copy_integer_adapter_rejects_out_of_range_value`,
  `dataframe::tests::copy_integer_element_enforces_int32_range`).
- **Code.** `crates/bn_rt/src/dataframe.rs` (`copy_integer_element`);
  `crates/bn_rt/src/dataframe_abi.rs` `bn_rt_dataframe_copy_integer`;
  `crates/bn_lib_data/src/lib.rs` `CopyIntegerColumn` adapter.

### K21 — Native fixed vectors: remaining gaps

- **Planned:** `ongoing/bucket-0.6.5.md`, section T (defects).

- **Resolved 2026-10-12: rows are no longer shared.** A fixed vector of two
  or more dimensions is a value natively: each binding, argument, call
  result, return buffer, default and `STATIC` owns one storage block (every
  row level, the elements, the dimensions), and `bn_rt_vector_nest` links
  it and copies the elements of the source, never its rows. Before, `b = a`
  shared rows and a returned vector pointed into the callee's dead frame
  (`--opt none`: exit 139). A default without initializer
  (`LET m AS INTEGER[2][2]`) was refused and now compiles. Evidence:
  `nested_vectors_are_values_on_both_backends`
  (`tests/parity_regressions.rs`, `tests/grammar/valid/nested-vector-values.bn`:
  2D and 3D, copy, argument, return, default, `STRING` elements, `STATIC`,
  1000 iterations), `bn_rt` `vector_abi` tests; object elements matched
  by hand on both backends (each released once).
- **Updated 2026-10-14 (Sprint 1 / V.S3):** class fields holding vectors are now supported with deep copy on `SetField`/`SetMember`; indexed `STATIC` stores are implemented (`SetStaticIndex`, resolving K65).
- **Resolved 2026-10-16:** storing a row into a nested vector (`g[0] = row`) is supported on `bnc` with value copy semantics into destination storage (without row pointer sharing), matching `bni`. Verified by `nested_vectors_are_values_on_both_backends` (`tests/parity_regressions.rs`).

### K22 — Native runtime strings are never freed — RESOLVED 2026-10-17

- **Resolved 2026-10-17 (Sprint 7 stage 1 completed; SP7.8, SP7.9, SP7.10).**
  Native STRING buffers freed at end of scope via `bn_rt_str_release`.
  Verified via the 100,000 iterations BN loop test (`line + "!"`) in
  `tests/compiler_parity.rs` (`hundred_thousand_string_concatenations_in_a_loop_releases_every_buffer`),
  `tests/ownership_matrix.rs` (including ASan on macOS and Windows MSVC), and
  `tests/arc_differential.rs`.
  Verified across both Unix (macOS) and Windows (`bn-windows`).
  Closes P.8 and T.K22.

### K32 — The numeric `AS` conversion table is not in the specification — DECIDED 2026-10-12

- **Decided 2026-10-12 (BDFL), confirmed in the spec on 2026-10-16.**
  `0.6.md` "Conversions with `AS`": a conversion to the same type is the
  identity; `NAN AS BOOLEAN` is `TRUE`; a finite `FLOAT64` beyond
  `FLOAT32` is `NUMERIC_OVERFLOW`; the `PRINT` text of a `TIMESTAMP` is
  its `INT64`. TODO (a)–(d) are not in the spec. The entry stays here
  until a close moves it. The historical note below is the state before
  that decision.

- **Done.** `0.6.md` "Conversions with `AS`" states the verified rules; the
  0.1 base is not on GitHub (first commit is 0.2) nor on this machine.
  Carlos decides its TODO (a)–(d). The other 0.1 areas: bucket section S.

- **Normative gap (Carlos decides).** `0.6.md` says "0.1 … conversions …
  unchanged", but the 0.1 archive was not recoverable (Status). The spec
  defines only C2, C3 (`AS STRING`) and literals in alternatives. Integer
  range checks, float truncation toward zero, `BOOLEAN` from numbers and
  strings, `NULL`/`NA`/`EOF` to `BOOLEAN`, the `PRINT` text of `TIMESTAMP`,
  `NAN AS BOOLEAN`, `FLOAT64` → `FLOAT32` overflow, and why `BOOLEAN AS
  BOOLEAN` / `STRING AS STRING` are refused come from
  `examples/conversions.bn` and `conversion_allowed`
  (`crates/bn_frontend/src/semantic/type_ops.rs`), not from the spec.
- Evidence: `tests/conformance/types/primitive-conversions.bn` (58 lines,
  expected output derived from those rules; `bni` matches every line,
  2026-10-09).

### K45 — `bni`: `Web.Client.New(logger)` contrato de logging — RESOLVED 2026-10-21

- **Resolved 2026-10-21.** `Client.New(logger)` keeps a `Log.Logger` (the same acceptance as `Server.New`) and writes one access record per `Request` and `RequestWithPolicy`. The shared field map is `bn_core_log::http_client_access`: method, path without the query, `HTTP/1.1`, status (`0` on transport failure), body size, `http.duration_ms`, and `http.header.<name>`. Sensitive keys, the query, and userinfo are omitted. `NEW Client()` stores no logger. `bni` looks the logger up at the call; `bnc` stores the handle in `bn_web_client_create` and `bn_rt_log_client_access` writes the record. Spec: `language/0.6/bnweb.md`, "Client and security policy".
- Evidence: `client_access_drops_the_query_userinfo_and_sensitive_headers` (`bn_core_log`); `http_client_logs_the_request_on_both_backends` (`tests/parity.rs`, `bni` and `bnc`, 2026-10-21); `bnweb_client_rejects_unsafe_urls_before_transport` still passes.

### K46 — `bni`: a network server with a logger writes no access record — RESOLVED 2026-10-17

- **Resolved 2026-10-17 (Section E.S0.g):** `ServerState` in `libraries/bn_web` keeps the logger (`Arc<Mutex<Logger>>`) and a thread-safe sink (`LogSink + Send + Sync`). `http.rs` (`serve_hyper_connection`) and `Server.Dispatch` both emit the access record (level `HTTP`, `http.method`, `http.path` without query, `http.status`, `request_id`) via `ServerState.log_access`. In `bni`, `bn_lib_web` wires `LogProvider::core_logger` to `ServerState` using `InterpLogSink`. Added logger to `examples/bnweb/users_api.bn`. Verified with unit tests (114 passing) and integration tests `bnweb_network_request_correlates_response_id_with_log_record` and `bnweb_users_api_example_authenticates_and_serves_users` on macOS and Windows host (`bn-windows`).

### K74 — `HOST.FileSystem` `ReadLine` reads one byte per system call — RESOLVED 2026-10-16

- **Resolved 2026-10-16:** `crates/bn_rt/src/file.rs` adds an 8 KB buffer to `OpenFile`, scanning lines in memory instead of 1 syscall per byte, while correctly maintaining the logical stream position across `read_line_bytes` and `read_all_text`. Verified by `cargo test -p bn_rt --lib` (140 tests pass).

### K77 — `bni lsp` allocates whatever `Content-Length` says — RESOLVED 2026-10-15 (Sprint 5)

- **Resolved 2026-10-15 (Sprint 5):** `crates/bn_lsp/src/lib.rs` implements bounded `Content-Length` framing over stdio (`read_bounded_message`), enforcing `MAX_DOCUMENT_BYTES` (8 MiB) before allocating byte buffers and rejecting oversized payloads immediately. Verified by integration test `tests/lsp_server.rs` (`k77_oversized_message_bounds_checked_without_unbounded_allocation`).
- **Severity: minor** (the input comes from the editor). Found 2026-10-13
  (N8). `Content-Length: 999999999999` kills `bni lsp` (status 137) while
  it allocates: `lsp-server` 0.10.0 `msg.rs` `buf.resize(size, 0)`. The DAP
  server bounds the message (`bn_dap` `MAX_MESSAGE_BYTES`); the LSP server
  now applies bounded framing with `MAX_DOCUMENT_BYTES`.

---

## Found by the 2026-10-11 corpus probe

Probe: 351 programs on `bni run` and the `bnc` binary (see
`bucket-0.6.5.md` section P). Each entry reproduced on a clean `HEAD`
(`8abb017`) worktree: all predate the working tree.

### K55 — `IS POINTER TO T` reads the allocation

Toolchain gap closed in the tree. Evidence, not re-run on 2026-10-16:
`pointer_is_reads_element_and_length` and
`nullable_integer_region_is_reads_length`. The entry was not moved.
The live `POINTER TO Box` arm stays out: `NEW Box()` is not
`POINTER TO Box`.

Recorded 2026-10-16 (BDFL decision) in the version table and History of
`0.6.md`. The paragraph under String indexing already stated the rule and
attributed it to 2026-10-11; that date was not a History entry. The rule
stands. `IS POINTER TO VOID` is every non-`NULL` pointer. Any other
`IS POINTER TO T` is true when the allocation's element is `T` and, when
the test names a length, that length. `POINTER TO E OR NULL` answers when
the tested type is compatible with the alternative: `VOID` for every
non-`NULL` value, and `E` or `E[n]` from the stored length. A test that is
not compatible with the alternative stays a static error. Class names are
not recovered from `POINTER TO VOID` (one shared element code).

`bnc` was refusing the length and `VOID` tests on a region alternative
(`TARGET_UNSUPPORTED_OP`). `emit_region_is` now answers them. The slot is
the same `{ ptr, i32 }` fat pointer as `POINTER TO T`; scope exit releases
the ARC header, not `free` of the data pointer. Evidence:
`nullable_integer_region_is_reads_length` (`pointer-region-is.bn`, both
backends, `TRUE TRUE TRUE FALSE FALSE FALSE FALSE FALSE FALSE TRUE`).
`pointer-is-allocation.bn` is unchanged.

The 2026-10-11 probe text below is the state before `pointer_is_holds` /
`emit_pointer_is`. It is not the current toolchain.

- `0.6.md` defines `IS POINTER TO VOID` ("true for every non-`NULL`
  pointer value"). For `IS POINTER TO T` with `T` not `VOID`, the probe of
  2026-10-11 found `bni` answering `TRUE` whenever the static types are
  compatible, without looking at the allocation: a `FLOAT64` region held in
  `POINTER TO VOID` is `IS POINTER TO INTEGER` = `TRUE`, and the conversion
  in the branch then stops with `POINTER_TYPE_MISMATCH`. Found 2026-10-11
  while fixing K51.
- **Cause in `bni`:** `crates/bn_interp/src/compare.rs` (the `IS` test of a
  name starting with `POINTER TO `) answers whether the value is a pointer
  at all; it never reads the element type or length the allocation keeps.
- **Carlos decides:** (A) `IS POINTER TO T` tests the element type (and,
  for a fixed length, the length) recorded in the allocation, so it is true
  exactly when the conversion succeeds; or (B) it is static only, and the
  spec says so. Until then `bnc` compiles only `IS POINTER TO VOID` on a
  pointer value and refuses every other pointer test with a support
  diagnostic (K51), including the pointer member of an alternative (K11).
- **Workpaper claim, not a recorded decision:** option (A) — `IS POINTER TO T`
  is `TRUE` when the allocation's element type is `T` (and, for
  `POINTER TO T[n]`, its length is `n`), `FALSE` otherwise and on `NULL`.
  The paragraph was later written into `0.6.md`. The decision is the
  History entry of 2026-10-16, not this workpaper.

### K58 — Diagnostics for code in an imported module name the entry file and internal type names — RESOLVED 2026-10-20

- **Resolved 2026-10-20.** The span already carried the imported module's
  `SourceId`. Presentation now selects that file
  (`source_of`) and rewrites `MODULE#<id>.Name` and `#<id>.Name` to the
  spelling of the diagnosing module (`L.Box` from an `IMPORT lib AS L`,
  `Box` inside the module that declares it). `bni check` of an entry
  that writes `LET box AS L.Box = 1` reports `Expected L.Box` at
  `main.bn`; a type error in the imported file is reported at `lib.bn`.
- **History.** Refusals were correct and the text misled: `main2.bn:8:12`
  for line 8 of `lib2.bn`, and internal names `MODULE#0.Box` / `#0.Box`.
  Found 2026-10-11.

### K62 — `bnc`: a vector literal in a loop grows the stack — RESOLVED 2026-10-17 (P.12)

- **Resolved 2026-10-17 (P.12 / K62):** Vector literal storage (`vecdata{dest}` and `vecstructcopy{dest}_{index}`) is pre-allocated in the function entry block (`b0`) in `crates/bn_llvm/src/llvm/functions.rs`, and `emit_vector` in `crates/bn_llvm/src/llvm/vectors.rs` reuses the entry slot without emitting in-loop `alloca` instructions. Verified via `tests/parity_regressions.rs` (`vector_literal_in_loop_reuses_entry_slot_without_stack_growth`).

### K63 — Other standard-library handles are never released — RESOLVED 2026-10-17

- **Resolved 2026-10-17:** Standard library handles (`BNJson.Json`, `BNData.DataFrame`,
  `BNSqlite.Connection`, `BNCrypto.Bytes`) implement counted handle retention (`retain`)
  and release (`release`/`close`) with holder tracking in their core crates (`bn_core_json`,
  `bn_core_sqlite`, `bn_lib_data`, `bn_lib_crypto`) and `bn_rt` C ABI (`bn_rt_json_retain`,
  `bn_rt_dataframe_retain`, `bn_rt_sqlite_retain`, `bn_rt_crypto_bytes_retain`).
  Declarations added to `crates/bn_llvm/src/llvm/runtime_abi.rs`. Verified by regression
  test `tests/parity_regressions.rs::standard_library_handles_are_released_at_end_of_binding`
  and 140 `bn_rt` unit tests.

### K64 — `bnc`: `IS Log.Logger` on `Log.Logger OR Error` — closed in the tree 2026-10-16

The measurement below already records `logger-alternative-is.bn` printing
`TRUE` then `FALSE` on both backends. The entry was not moved. `IS` on
`Dispatch.Queue OR Error` is K87 and stays open.

- **Severity: gap** on the 2026-10-11 probe (refused with a support
  diagnostic). `IF made IS Log.Logger THEN` was `TARGET_UNSUPPORTED_OP`
  ("IS Logger on a value of type 'MODULE#0.Logger OR Error' has no LLVM
  lowering yet"); `IS Error` worked. Found 2026-10-11. The non-standard
  imported class (`IS Dispatch.Queue`) is K87, not this entry.
- **Measured 2026-10-16 (Sprint 4 phase 1).** `alternative_is` accepts only
  the name lowering emits: the simple name of a standard module, or
  `#<module>.<Name>`. An alias (`Log.Logger` as the test string) and a
  foreign suffix (`Foo.Logger`) do not match. `logger-alternative-is.bn`
  prints `TRUE` then `FALSE` on `bni` and `bnc`
  (`imported_logger_alternative_is_the_logger`). Unit:
  `imported_alternative_matches_only_the_names_lowering_emits`.

### K65 — `bnc`: indexed assignment to a `STATIC` or a member vector — RESOLVED 2026-10-14 (Sprint 1 / V.S3)

- **Resolved 2026-10-14 (Sprint 1 / V.S3):** Implemented `Instruction::SetStaticIndex` and `Instruction::SetMemberIndex` lowering in `crates/bn_llvm/src/llvm/indexed_store.rs` with bounds checking, trap reporting, and proper vector fat pointer updates. Verified by `tests/compiler_parity.rs` (`indexed_struct_and_static_fields_preserve_language_semantics`).


### K66 — wasm32: endless recursion stops with the engine's message, not `STACK_OVERFLOW` — RESOLVED 2026-10-20 (target withdrawn)

- **Resolved 2026-10-20 (BDFL).** The wasm32 target is withdrawn, so the
  divergence has no remaining backend. Native and `bni` already agree on
  `STACK_OVERFLOW`.
- **History.** On wasm32 the engine stopped recursion with its own message.
  A function whose locals stayed in wasm locals never moved the shadow
  stack, so `bn_rt_stack_check` in `bin/bn-wasm` never tripped. Found
  2026-10-12. The host is gone with the target.

### K81 — DAP: Comandos de controle de execução não implementados — RESOLVED 2026-10-15 (Sprint 5)

- **Resolved 2026-10-15 (Sprint 5):** `crates/bn_dap/src/lib.rs` implementa o despachante de controle de execução (`next`, `stepIn`, `stepOut`, `pause`, `continue`) com sincronização via condvar, rastreamento de profundidade de pilha em `stepOut`, transição de linha em IR intra-linha, e respeito estrito ao parâmetro `stopOnEntry` da requisição `launch`. Testado em `crates/bn_dap/tests/dap_session.rs`.
- **Severity: gap**.

### K82 — LSP: Métodos de edição, refatoração e dobramento retornam method not implemented (normative / clarified)

- **Clarificação Normativa (Carlos, 2026-10-15):** A especificação `language/0.6/ide-tooling.md` é normativa e define que o formatador está explicitamente fora do escopo da linguagem ("a formatter [is] outside") e métodos não implementados devem retornar erro JSON-RPC `-32601` (`MethodNotFound`), preservando a integridade do código contra heurísticas textuais destrutivas. Métodos opcionais de mutação de código (`formatting`, `rename`, `codeAction`) retornam `-32601` conforme a especificação exige. O suporte a `textDocument/foldingRange` foi implementado estritamente baseado nos nós da AST real (`Statement::If`, `While`, `Repeat`, `For`, declarações de rotinas/membros) sem uso de heurísticas por expressões regulares.
- **Severity: normative behavior (not a defect)**.

### K85 — `bnc`: ownership of a `STRING` returned by a user function (open)

- **Severity: high** (a fresh string can leak; releasing a literal can fault).
  Updated 2026-10-16. Not closed.
- **Sprint 6 subset, then removed.** On 2026-10-16 a function yielded an
  owned string only when every `RETURN` was `+` of strings, `TOLOWER`,
  `TOUPPER`, `CHAR`, or a call of such a function. The caller released
  that pointer once. A returned literal was not released.
  `user-calls.bn` printed `5`, `pardal`, `papa`, `papa` on both backends
  (`sprint6_calls_vectors_and_steps_match_the_interpreter`). Sprint 7
  deleted `owned_user_strings` and `owned_string_results`. Ownership is
  now the IR retain/release of G7-D5. A literal is an immortal header, so
  a release of it is a no-op. `bn_rt_str_release` of null is a no-op.
- **Still open.** A `RETURN` of a local, a parameter, or a mix of a
  literal and a fresh string was not retested after that deletion.
  `user-calls.bn` was not run again. The old fear (a release reading 32
  bytes before a `.rodata` literal) does not apply to a header literal;
  the shapes themselves are still untested.

### K86 — Arquitetura: Segregação do BNSqlite em Crate Autônoma e Modularização do Backend LLVM — RESOLVIDO PARCIALMENTE / PROPOSTA PÓS-0.6.5

- **Resolved (SQLite segregation, 2026-10-14):** Commit `79f9910` segregou toda a lógica de domínio e decodificação do SQLite em `crates/bn_core_sqlite`, desacoplando runtime e interpretador (`9d66472`).
- **Deferred past 0.6.5 (LLVM backend partition):** A divisão de `crates/bn_llvm` em quatro sub-crates (`bn_llvm_ir`, `bn_llvm_layout`, `bn_llvm_arc`, `bn_llvm_ops`) permanece documentada na proposta `todo/proposals/llvm-backend-modularization-and-sqlite.md` para execução pós-0.6.5.

### K87 — `bnc`: `IS` on an alternative of an imported module class has no lowering (open)

- **Severity: medium** (a program `bni` runs is refused by `bnc`; no wrong
  result). Split from K44 on 2026-10-15, when K44's isolation defect closed.
- `IF queue IS Dispatch.Queue` on `Dispatch.Queue OR Error` stops the build
  with `TARGET_UNSUPPORTED_OP` ("IS Queue on a value of type
  'MODULE#0.Queue OR Error' has no LLVM lowering yet"); `bni` prints the
  branch. The same gap as the open `bnc` part of K41 (`IS` on
  `Str.Tokenizer OR NULL`). `IS Error` on the same value lowers.
- Reproduced 2026-10-15 on the working tree (probe:
  `queue IS Dispatch.Queue` after `Dispatch.Queue.Serial()`).

### K89 — `bnc`: destroying an object does not free its fixed-vector fields — RESOLVED 2026-10-17

- **Resolved 2026-10-17:** Emitted class destructor in `crates/bn_llvm/src/llvm/arc_runtime.rs`
  inspects class metadata for `vector_field_offsets` and emits `free(buf)` for each fixed-vector field.
  Verified by 2,000-round loop with constant memory footprint and formal regression test
  `tests/heap.rs::object_destruction_releases_fixed_vector_element_buffer`.

### K91 — `bnc`: storing a narrowed resource into `T OR NULL` (or a logger `STATIC`) fails code generation — RESOLVED 2026-10-17

- **Resolved 2026-10-17:** Emitted storage logic in `crates/bn_llvm/src/llvm/emission1.rs` invokes `coerce_to_type` whenever `value_llvm != slot_llvm`. `coerce_to_type` in `crates/bn_llvm/src/llvm/helpers.rs` and `wrap_success` in `crates/bn_llvm/src/llvm/alternatives.rs` handle conversion between `{ i1, ptr, i64 }` and `Ptr` for both integer resource handles (`FS.File`, `Net.TCPListener`, `BNSqlite.Connection`, `DataFrame`, `BNDispatch` primitives) and pointer resources (`Log.Logger`). Verified by `tests/parity_regressions.rs::narrowed_resources_store_into_nullable_and_error_alternatives`.

### K92 — `BNDispatch` coordination values cannot reach a task — RESOLVED 2026-10-17

- **Resolved 2026-10-17 (P.6c):** Coordination values (`Queue`, `Ticket`, `Group`, `Barrier`, `Semaphore`, `Mutex`) and temporal types (`DATE`, `TIME`, `TIMESTAMP`) cross into tasks seamlessly on both backends. `bni` uses process-wide `SHARED_DISPATCH`; `bn_rt` holds process-wide tables with mutex auto-unlock on worker completion; `bnc` dispatch trampoline unpacks handles and correctly converts `i32` `DATE`/`TIME` and `i64` `TIMESTAMP`. Incompatible parameter types (objects, vectors, structs, alternatives, other handles) are statically rejected with `TYPE_MISMATCH`. Verified by `build_typed_dispatch_supports_handle_arguments_and_concurrency` (`tests/parity.rs`, `tests/grammar/valid/dispatch-handle-arguments.bn`) and `async_function_parameter_count_and_types_are_validated` (`tests/semantic.rs`).

### K93 — `BNSqlite`: ausência de parâmetros vinculados (bound parameters / prepared statements)

- **Severity: medium / normative gap**.
- `BNSqlite` não possui suporte a parâmetros vinculados preparados (`?`, `$1`). A construção de consultas com entradas dinâmicas dependeria de interpolação textual, o que viola diretrizes de segurança contra injeção de SQL.
- **Decisão BDFL (2026-10-17):** Item `E.S7.d` (exemplo didático do livro de serviço REST CRUD com `BNSqlite`) retirado do escopo de 0.6.5. O exemplo será reintroduzido quando a sintaxe normativa de parâmetros vinculados for especificada para `BNSqlite`.
