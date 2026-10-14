# Known issues — Basic Next 0.6.5 working tree (open; updated 2026-10-14)

`issues.md` was retired on 2026-10-13 after migration here. Every still-open
finding it contained is already represented by an entry below: BNWeb native
(K14), BN functions of provider modules (K48, superseded by P.15), SQLite
rowid (K73), `HOST.FileSystem` `ReadLine` (K74), SQLite import diagnostics
and empty BLOB (K76, K78), DAP/LSP missing protocol commands, and the product
hygiene debts already tracked by the H section. The stale, resolved native HOST
and IR entries were not migrated.

Only open issues are listed. Resolved entries, with their cause, fix and
evidence, moved on 2026-10-11 and 2026-10-14 to `done/known_issues-0.6.5-resolved.md`
(K2, K3, K4–K9, K13, K18, K19, K23–K30, K33–K43, K47–K53, K56, K57, K59, K61,
K25, K67, K68, K70, K71, K72, K75, K79, K80); the bucket still cites them by number.

First measured (2026-10-09) on branch `bucket-0.6.5-corrections` plus the uncommitted 0.6.5c
work. Corpus probe: the 213 programs of `examples/` and
`tests/grammar/valid/`: `bni check` passes on all 213; `bnc` builds 203; 9
stop with a support diagnostic, 1 with a defect (K4). Wasm: 9 of a 40-program
sample run under `bin/bn-wasm`.

Severity: **critical** (security or data integrity), **high** (wrong
behaviour), **gap** (declared native support gap: refused with a support
diagnostic, never a wrong result), **minor**. "Since" says whether the issue
predates this session (checked on a clean `HEAD` worktree where marked).

**Bar (Carlos, 2026-10-09):** 0.6.5 is a production release without errors.
Every open entry here, gaps included, is fixed before the `v0.6.5` tag;
interim fixes (K1, K3) do not count. Each entry names its 0.6.5 bucket.

---

## Critical

### K1 — A `STRING` holding NUL stops a native program (denial of service)

- **Symptom.** Any native program that reads external text (file, `INPUT`,
  process output, `BNJson`, `BNSqlite`/`BNData`) stops with
  `TARGET_UNSUPPORTED_TYPE` as soon as the data contains U+0000. Whoever
  controls the data controls availability. `bni` keeps the character.
- **History.** Before 0.6.5b S7.c (option C) the native value was cut at
  the NUL in silence: `"admin\0x"` became `"admin"`, an integrity flaw
  that can defeat comparisons. Option C traded it for the stop.
- **Root cause.** A native `STRING` is a NUL-terminated C string: the LLVM
  type is a bare `ptr` and every `bn_rt` string ABI takes or returns
  `*const c_char` / `*mut c_char`. The length is recomputed by scanning for
  the terminator, so a NUL inside the value is indistinguishable from the
  end.
- **Code.**
  - stop: `crates/bn_rt/src/text_abi.rs` (`reject_nul`, `c_string_or_stop`,
    `owned_c_string`, `bn_rt_input_nul`) and its callers (`lib.rs`
    `bn_rt_str_char_utf8`, `file_abi.rs` read functions, `exec.rs`,
    `json_abi.rs` getters, `dataframe_abi.rs` `GetString`);
  - representation: `crates/bn_llvm/src/lib.rs` `llvm_type` (STRING → `ptr`),
    the emitted `@bn_input` (`crates/bn_llvm/src/llvm/helpers.rs`
    `input_runtime_ir`), the length scan (`@bn_rt_str_len` /
    `length.scan` in `helpers.rs`);
  - ABI list: `docs/architecture/value-memory-abi.md`.
- **Planned:** `ongoing/bucket-0.6.5.md` G.S1.
- **Fix.** Option A: a native `STRING` carries its length (`{ ptr, len }`)
  across the emitted code and the `bn_rt` ABI, so NUL is an ordinary
  character as in `bni`. Large: every string ABI and the `bn_llvm` string
  paths. Separate bucket.
- **Since.** Truncation predates this session; the stop since `2c0b6ea`.

---

## High

### K69 — `bnc`: run-time `STEP 0` and `STOP` out of range (open)

- **Severity: wrong result** (native). Found 2026-10-12.
  - A `FOR` whose step is `0` at run time skips the loop natively; `bni`
    stops with `INVALID_FOR_STEP` (spec: "Control flow").
  - `STOP c` with `c = -1` exits with status `255` natively; `bni` stops
    with `INVALID_EXIT_CODE`.

---

## Native support gaps (refused with a support diagnostic)

### K10 — Store into a `STRUCT` local declared without an initializer — RESOLVED 2026-10-14 (Sprint 1 / V.S3)

- **Resolved 2026-10-14 (Sprint 1 / V.S3):** Value witnesses (`@__bn_copy_*` and `@__bn_destroy_*`) synthesize deep copy and destruction for structs holding vectors and nested structs. `Instruction::Store` and indexed stores perform deep copy with drop glue on existing slots. Verified in `tests/value_matrix.rs` (41/41 cells clean) and `tests/compiler_parity.rs` (`indexed_struct_and_static_fields_preserve_language_semantics`).


### K11 — `IS POINTER TO T` on an alternative

- **Planned:** `ongoing/bucket-0.6.5.md`, section T (defects).

- `tests/grammar/valid/pointer-named-type.bn` line 17
  (`value IS POINTER TO Box`): `IS on this alternative`.
- **Root cause.** `emit_is` has no test for a pointer member of a general
  alternative.
- **Code.** `crates/bn_llvm/src/llvm/emission1.rs` 605–620 (`emit_is`
  call); `crates/bn_llvm/src/llvm/alternatives.rs`.

### K12 — Local vector whose dimension is an expression

- **Planned:** `ongoing/bucket-0.6.5.md`, section T (defects).

- `tests/grammar/valid/local-vector-expression-dimension.bn`
  (`LET values AS INTEGER[n]`): `default value of 'INTEGER(INT32)[]' with
  dimensions is unavailable`.
- **Root cause.** `Default` with dynamic dimensions has no lowering
  (fixed-size slots are allocated at function entry).
- **Code.** `crates/bn_llvm/src/llvm/helpers.rs` 903–910.

### K14 — `BNWeb` under `bnc`

- Every `BNWeb` call is `TARGET_UNSUPPORTED_OP` natively. Bucket
  `bucket-0.6.5.md` (section E).

### K15 — Wasm runs only a subset

- **Symptom.** 9 of 40 sampled fixtures run; most fail at instantiation
  (`function import requires a callable`), including `HOST.Env`
  (`bn_rt_env_get` / `_has`).
- **Root cause.** `bin/bn-wasm` re-implements part of `bn_rt` in
  JavaScript; `bn_rt` cannot be built for `wasm32` (unconditional
  `socket2`, `dns-lookup`, `rusqlite`, crypto crates).
- **Code.** `bin/bn-wasm`; `crates/bn_rt/Cargo.toml`. Bucket 0.6.5c S7.

---

## Divergences and minor issues

### K16 — `BN_ARC_TRACE` labels a region differently

- `bni`: `arc new Float64#0 …`; native: `arc new #0 …` (counts, ids and
  order match). Registered in `bucket-0.6.5.md` (section T).
- **Root cause.** The native trace does not receive the element type of a
  region allocation. TODO: confirm the emitting site.
- **Code.** `crates/bn_rt/src/arc.rs` 149–238 (trace); the interpreter
  label in `bn_interp` (TODO: locate).

### K17 — `CopyIntegerColumn` range check differs

- **Planned:** `ongoing/bucket-0.6.5.md` C.S8.c.

- Native returns `INVALID_FORMAT` "integer out of range" for a cell beyond
  `INT32`; `bni` copies the value without the check. TODO: reproduce with a
  64-bit value.
- **Code.** `crates/bn_rt/src/dataframe_abi.rs` `bn_rt_dataframe_copy_integer`
  (`i32::try_from`); `crates/bn_lib_data/src/lib.rs` `CopyIntegerColumn`
  adapter.

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
- **Open:** storing a row into a nested vector (`g[0] = row`) is refused with a support diagnostic (no sharing).

### K22 — Native runtime strings are never freed

- **Planned:** `ongoing/bucket-0.6.5.md`, section T (defects).

- `line + "!"` in a loop grows memory (16.8 MB measured, also on `HEAD`):
  native string ownership is not defined. Open in
  `bucket-0.6.5.md` (section T). Related to K1 (same representation).

### K32 — The numeric `AS` conversion table is not in the specification — SPEC WRITTEN 2026-10-09 (TODO a–d open)

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

### K44 — `bnc`: an `ASYNC` task shares the program's `STATIC` fields (open)

- **Severity: high** (backends differ; specification broken under `bnc`).
  `0.6.md`, "Isolation and conformance": a worker runs on an isolated
  module copy. A task that writes `Counter.hits` (set to 10 by `Start`):
  `bni` prints `task sees 1` then `main sees 10`; the `bnc` binary prints
  `task sees 11` then `main sees 11`. Found 2026-10-10 measuring task
  isolation for BNWeb handlers under `bnc` (E), which need the same
  isolation.
- **Also seen:** `IS Dispatch.Queue` on `Dispatch.Queue OR Error` is
  `TARGET_UNSUPPORTED_OP` ("IS on this alternative") under `bnc`, as in K41.

### K45 — `bni`: `Web.Client.New(logger)` rejects every logger (Carlos decides)

- `Client.New(logger AS Log.Logger)` fails with `TYPE_MISMATCH` ("non-logger
  value") for a logger from `Log.Logger.New`: the adapter accepts only a
  plain object, and a logger is a provider handle. Even if accepted, the
  adapter would ignore the logger. Only `NEW Web.Client()` is tested.
- **Normative gap.** `bnweb.md` does not say what the client logs through
  the logger (requests? failures? which fields?). Decide before E closes:
  specify the records, or drop `Client.New(logger)` from `BNWeb.bn`.

### K46 — `bni`: a network server with a logger writes no access record (open)

- `Web.Server.New(logger)` keeps the logger, but only the in-process
  `Server.Dispatch` writes the access record (`log_web_dispatch`); requests
  served over HTTP (a `200` and a `404` in the probe) write nothing.
  `bnweb.md`: "access/error records through `BNLog`". Found 2026-10-10
  adding a logger to `examples/bnweb/users_api.bn` (Carlos).
- **Progress 2026-10-10.** Step 1 done: one `BNLog.Logger` in the shared
  core (`bn_core_log::Logger`, `LogSink`, `Logger::write`); `bni`
  (`bn_lib_log`, `CoreSink`) and `bnc` (`bn_rt` `log_abi`, `RuntimeSink`)
  dropped their own copies, so a record too long now fails with the same
  message on both. `LogProvider::core_logger` hands the shared logger to
  another library. Evidence: `bn_core_log` logger unit tests,
  `host_conformance` (bni × bnc), `runtime` `bnlog_*` and `bnweb_*`,
  `shared_logic`. Open: `ServerState` keeps the logger and `http.rs`
  writes the access record (level `HTTP`; method, path without query,
  status, request id) after every response, `Server.Dispatch` through the
  same code; the `bn_rt` registry holds `Arc<Mutex<Logger>>` when
  `web_abi` needs it (E.S4); then the logger in `users_api.bn`.

### K74 — `HOST.FileSystem` `ReadLine` reads one byte per system call (open)

- **Severity: minor** (performance, both backends). Found 2026-10-13
  (audit `audit/2026-10-13-auditoria.md`, N5); `HEAD` `e23ac62`.
  `bn_rt/src/file.rs` `read_line_bytes` (a `ponytail:` marker) reads with a
  one-byte buffer. Counting the lines of a 16 MB file (200 000 lines),
  release builds: `bni` 10.5 s, `bnc` 10.2 s (7.7 s of them in the
  kernel); `wc -l` 0.03 s.
- **To fix:** a buffered reader per open file that keeps the logical
  position for the next `Read*` call (the reason the comment gives).

### K77 — `bni lsp` allocates whatever `Content-Length` says (open)

- **Severity: minor** (the input comes from the editor). Found 2026-10-13
  (N8). `Content-Length: 999999999999` kills `bni lsp` (status 137) while
  it allocates: `lsp-server` 0.10.0 `msg.rs` `buf.resize(size, 0)`. The DAP
  server bounds the message (`bn_dap` `MAX_MESSAGE_BYTES`); the LSP server
  should read the header itself and apply the same bound.

---

## Found by the 2026-10-11 corpus probe

Probe: 351 programs on `bni run` and the `bnc` binary (see
`bucket-0.6.5.md` section P). Each entry reproduced on a clean `HEAD`
(`8abb017`) worktree: all predate the working tree.

### K55 — `IS POINTER TO T` on an opaque pointer: what it tests (decided, open)

- `0.6.md` defines only `IS POINTER TO VOID` ("true for every non-`NULL`
  pointer value"). For `IS POINTER TO T` with `T` not `VOID`, `bni`
  answers `TRUE` whenever the static types are compatible, without looking
  at the allocation: a `FLOAT64` region held in `POINTER TO VOID` is
  `IS POINTER TO INTEGER` = `TRUE`, and the conversion in the branch then
  stops with `POINTER_TYPE_MISMATCH` — the test cannot guard the
  conversion. Found 2026-10-11 while fixing K51.
- **Cause in `bni`:** `crates/bn_interp/src/compare.rs` (the `IS` test of a
  name starting with `POINTER TO `) answers whether the value is a pointer
  at all; it never reads the element type or length the allocation keeps.
- **Carlos decides:** (A) `IS POINTER TO T` tests the element type (and,
  for a fixed length, the length) recorded in the allocation, so it is true
  exactly when the conversion succeeds; or (B) it is static only, and the
  spec says so. Until then `bnc` compiles only `IS POINTER TO VOID` on a
  pointer value and refuses every other pointer test with a support
  diagnostic (K51), including the pointer member of an alternative (K11).
- **Decision (Carlos, 2026-10-11):** option (A). `IS POINTER TO T` is
  `TRUE` when the allocation's element type is `T` (and, for
  `POINTER TO T[n]`, its length is `n`), `FALSE` otherwise and on `NULL`;
  it holds exactly when the conversion succeeds. Spec: `0.6.md`, the
  `POINTER TO VOID` paragraph. Open: `bni` (`compare.rs`) must read the
  allocation; `bnc` must emit the same test (the region header keeps the
  element type and length — TODO: confirm in `bn_rt`) instead of the
  support refusal of K51.

### K58 — Diagnostics for code in an imported module name the entry file and internal type names (open)

- **Severity: minor** (refusals and errors are correct; their text
  misleads). A diagnostic raised in a function of an imported module
  points to the entry file with the module's line and column
  (`main2.bn:8:12` for line 8 of `lib2.bn`; also the K57 `INVALID_IR`
  pointed at `main.bn:27:1`), and support texts show internal names:
  `POINTER TO #0.Box OR NULL` for module code, `POINTER TO MODULE#0.Box`
  and the test `POINTER.TO.L.Box` for the same type seen from the entry
  module. Found 2026-10-11 while checking K57; the file attribution is the
  same on a clean `HEAD`.
- **To fix:** spans of imported code carry their own `SourceId` through
  lowering (AGENTS.md: "Retain SourceId + spans"), and type texts in
  diagnostics use the source spelling (`Box`, `L.Box`).

### K62 — `bnc`: a vector literal in a loop grows the stack (open)

- **Severity: minor** (stack use grows with loop turns; a long loop can
  overflow). Found 2026-10-11 while fixing K13. A vector literal's storage
  (`vecdata`, and `vecstructcopy` for STRUCT elements) is an `alloca`
  emitted where the literal is (`bn_llvm` `vectors.rs`), not at function
  entry, so each turn of a loop reserves new stack. TODO: confirm with a
  loop of 10,000 turns on a small stack.
- **To fix:** reserve a literal's storage at function entry, as K13 did for
  STRUCT bindings.

### K63 — Other standard-library handles are never released (open)

- **Severity: minor** (resources held until the program ends, no wrong
  result). Same root cause as K61 (found 2026-10-11 while fixing it): the
  IR counts no handle of a natively implemented library but
  `BNLog.Logger`. A `BNJson.Json` from `Json.Json.Parse` in a function is
  never released (no `Release` in the IR, no `bn_rt_json_release` in the
  native code); `BNData.DataFrame`, `BNSqlite` connections and the others
  are the same unless the program uses `RELEASE`.
- **To fix:** extend the counted classes (`bn_ir::names`) library by
  library, each with `retain`/`release` in its provider and in `bn_rt`, as
  K61 did for the logger.

### K64 — `bnc`: `IS Log.Logger` on `Log.Logger OR Error` (open)

- **Severity: gap** (refused with a support diagnostic). `IF made IS
  Log.Logger THEN` is `TARGET_UNSUPPORTED_OP` ("IS Logger on a value of type
  'MODULE#0.Logger OR Error' has no LLVM lowering yet"); `IS Error` works.
  Same family as the `IS Dispatch.Queue` gap noted in K44. Found 2026-10-11.

### K65 — `bnc`: indexed assignment to a `STATIC` or a member vector — RESOLVED 2026-10-14 (Sprint 1 / V.S3)

- **Resolved 2026-10-14 (Sprint 1 / V.S3):** Implemented `Instruction::SetStaticIndex` and `Instruction::SetMemberIndex` lowering in `crates/bn_llvm/src/llvm/indexed_store.rs` with bounds checking, trap reporting, and proper vector fat pointer updates. Verified by `tests/compiler_parity.rs` (`indexed_struct_and_static_fields_preserve_language_semantics`).


### K66 — wasm32: endless recursion stops with the engine's message, not `STACK_OVERFLOW` (open)

- **Severity: divergence** (wasm32 only; native and `bni` agree).
  `tests/runtime-traps/stack-overflow.bn` on wasm32 prints `before` and
  then `bn-wasm: Maximum call stack size exceeded` (exit 2) instead of
  `error[STACK_OVERFLOW]` (exit 1). A function whose locals stay in wasm
  locals never moves the shadow stack, so `llvm.frameaddress` does not
  grow and `bn_rt_stack_check` (in `bin/bn-wasm`, against `__stack_low`)
  never trips; the engine's own call-depth limit stops it first. Same
  before K53. Found 2026-10-12 by the audit of K21; reproduced again at the
  T.5 close on a clean build of `cdd56b3` (same message, exit 2).
- **Related, fixed 2026-10-12 (T.5 close, Windows host):** the check read
  `__data_end` and assumed the stack sits above the data; the Windows
  `wasm-ld` placed the stack first, so every frame looked overflowed and
  any wasm32 program with a user call exited 1 without output
  (`user_calls_and_nested_vectors_match_interpreter` failed there).
  `bnc` now links with `--stack-first` on every host and exports
  `__stack_low`, which `bin/bn-wasm` checks against.
- **To fix:** a depth the emitted code counts itself on wasm32, or the host
  turning the engine's `RangeError` into the `STACK_OVERFLOW` report (the
  callee name must reach the host).

### K81 — DAP: Comandos de controle de execução não implementados (open)

- **Severity: gap**.
- Em `crates/bn_dap/src/lib.rs`, os comandos padrão de controle de depuração (`next`, `stepIn`, `stepOut`, `continue`, `pause`, `restart`) caem na cláusula curinga e retornam `"request is not implemented"`.

### K82 — LSP: Métodos de edição, refatoração e dobramento retornam method not implemented (open)

- **Severity: gap**.
- Em `crates/bn_lsp/src/lib.rs`, requisições usuais do protocolo como formatação (`textDocument/formatting`), renomeação (`textDocument/rename`), ações de código (`textDocument/codeAction`) e dobramento de blocos (`textDocument/foldingRange`) retornam erro JSON-RPC `-32601` (`method not implemented`).

### K83 — Inconsistência de resolução de chamadas a alias de funções HOST importadas (open)

- **Severity: wrong result / trap**.
- A instrução `IMPORT HOST.NumProcs AS N` seguida da chamada `N()` passa pela checagem estática (`bni check`), mas falha com `UNINITIALIZED_VALUE` no interpretador e finaliza com sinal 133 no binário nativo compilado.

### K84 — bnc: Desenrolamento excessivo de laço na comparação de vetores longos (open)

- **Severity: minor** (code bloat / performance do compilador).
- Em `crates/bn_llvm/src/llvm/composite_compare.rs`, a comparação de igualdade de vetores desenrola inline todos os elementos do bloco (`for idx in 0..count`). Para vetores com muitos elementos (ex: `INT32[1000]`), são emitidas milhares de instruções SSA no mesmo bloco básico. Otimização planejada: para vetores com mais de 16 elementos, utilizar `memcmp` em escalares inteiros e laço compacto com nó `phi` e *short-circuit* em tipos complexos.

### K85 — bnc: Rejeição de chamadas gerais a funções de usuário não-inlined (open)

- **Severity: gap** (refused with support diagnostic).
- O compilador nativo não suporta chamadas a funções gerais de usuário que não sejam inlined (`crates/bn_llvm/src/llvm/helpers.rs` `unsupported_call_detail`), barrando a compilação com mensagem orientando a utilizar o interpretador.

### K86 — Arquitetura: Segregação do BNSqlite em Crate Autônoma e Modularização do Backend LLVM (open)

- **Severity: architectural debt / Clean Code**.
- **Diretriz do BDFL (Carlos, 2026-10-14)**: BNSqlite é uma biblioteca utilitária e não faz parte do core da linguagem BN. A estrutura do compilador e de bibliotecas requer particionamento estrito em crates especializadas:
  - **BNSqlite**: isolar a lógica e decodificação em `crates/bn_core_sqlite`, mantendo `bn_lib_sqlite` (adaptador do interpretador) e `bn_rt/src/sqlite_abi.rs` como adaptadores finos sem acoplamento ao compilador.
  - **Backend LLVM (`crates/bn_llvm`)**: particionar a crate atual de 25.500 linhas em quatro sub-crates de responsabilidade única:
    - `bn_llvm_ir`: representação e emissão do IR LLVM em texto.
    - `bn_llvm_layout`: cálculo de offsets, alinhamento, tipos e ABI de valores.
    - `bn_llvm_arc`: geração de witnesses de cópia/destruição e ARC runtime glue.
    - `bn_llvm_ops`: operadores nativos (vetores, dispatch, chamadas externas).
- **Proposta formal**: `todo/proposals/llvm-backend-modularization-and-sqlite.md`.
