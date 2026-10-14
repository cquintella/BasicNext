# Bucket 0.6.6 — BNFfi: C calls through a standard module

**Status:** Planned (todo). Not started.
**Tag:** `docs/releases/0.6.6.md` on close.
**Proposal:** [`todo/proposals/bnffi.md`](proposals/bnffi.md)

**Claim:** a Basic Next program calls a precompiled C function through the
standard module `BNFfi`, with the same observable result under `bni run` and
native `bnc`, and **no change to the language** (`language/0.6/0.6.ebnf`,
`0.6.md`, `keywords.md` unchanged).

**Prerequisites (must be closed before S2; none is closed on 2026-10-07):**

- [`bucket-0.6.5a.md`](bucket-0.6.5a.md): class objects passed to interface
  parameters (`args.Add(NEW C.Double(…))`, `C.Call(…, r)`), and
  standard-module classes in the IR class model (`C.Double IMPLEMENTS
  C.Value`).
- [`bucket-0.6.5b.md`](bucket-0.6.5b.md): `HOST.Env` (`bn_host_env`), through
  which `BN_LIBRARY_PATH` is read.
- [`bucket-0.6.5c.md`](bucket-0.6.5c.md): minimal `bni`/`bnc` (the `--ffi-path`
  option of S4.b is declared in the tool descriptor, not in `bni` `main.rs`).
  The option-ownership part of S4.b moved to
  [`bucket-0.6.5g.md`](../ongoing/bucket-0.6.5g.md) S2 (2026-10-09): `--ffi-path` is
  declared through it, so 0.6.5g closes before this bucket.
  It also fixes the core/adapter rule that D7 depends on.
- [`bucket-0.6.5e-bnweb-native.md`](bucket-0.6.5e-bnweb-native.md): BNWeb
  under `bnc` (defect fix, ordered before this bucket on 2026-10-08). If its
  D3 is accepted, it builds the shared standard-module emission table that
  S5.b reuses, and it consumes part of the `bn_llvm` budget: measure it again
  at S0.

`bucket-0.6.5a.md` exists in both `todo/` and `ongoing/` with different
contents; the `ongoing/` copy is the live one. TODO (Carlos): remove the
`todo/` copy.

**Not here:** any grammar, keyword or semantic rule; `EXTERN`; ARC changes;
structs by value, callbacks, variadic functions, C `NULL` as an argument,
`long` / `size_t` arrays; reading C `errno`; methods on `C.Value` (it is a
marker interface, so the `bnc` interface-call gap excluded by 0.6.5a never
arises); Wasm (support diagnostic only); linking static C
libraries (`.a` / `.lib`; decided 2026-10-07, Carlos: dynamic libraries only);
exporting Basic Next functions to C (language change; see
[`proposals/c-export.md`](proposals/c-export.md)).

---

## 1. Technical analysis

### 1.1 How a native standard module works today (the `BNSqlite` path)

| Layer | `BNSqlite` | `BNFfi` |
| --- | --- | --- |
| Surface | `modules/bn/BNSqlite.bn`: exported classes with stub bodies | `modules/bn/BNFfi.bn`, same form |
| Recognition | `StandardModule::BNSqlite` (`module_graph.rs` 25–34); `Module.bnsqlite_providers` | `StandardModule::BNFfi`; `bnffi_providers` |
| Lowering | standard modules are not lowered (`lowering.rs` 376–378) | same; the class model of their declarations comes from 0.6.5a S3.a |
| Interpreter | `bn_lib_sqlite::SqliteProvider` (feature `lib-sqlite`), registered in `bn_interpret_driver/src/libraries.rs` 53–56; receives `CONSTRUCTOR` with its arguments and every method; `object_destroyed` frees native state | `bn_lib_ffi::FfiProvider` (feature `lib-ffi`) |
| Native runtime | `bn_rt/src/sqlite_abi.rs` (C ABI), `rusqlite` with `bundled` C | `bn_rt/src/ffi.rs` + `ffi_abi.rs`, `libffi` with bundled C |
| Emission | `bn_llvm/src/llvm/sqlite_ops.rs` (149 lines, per method) | one table-driven path (budget) |
| Interpreter → runtime | `bn_lib_sqlite` depends on `bn_rt` and calls its Rust API | `bn_lib_ffi` calls `bn_rt::ffi` (the one implementation) |

### 1.2 Facts that shape the design

- **No standard-module class has a field today**, and lowering skips
  standard-module layouts: a public field such as `r.Value` has no layout to
  read on either backend. Methods are the proven path (`DataFrame`,
  `Connection`). → accessor methods (D6).
- A provider receives `CONSTRUCTOR` with its arguments (`bn_lib_sqlite/src/lib.rs`
  40–51), so `NEW C.Double(10.0)` is supported by the interpreter model.
- Native standard-module objects are `bn_rt` handles, not class-header objects
  (`is_bndata_dataframe_type` excludes them from the header store,
  `value_emission.rs` 138–147); their lifetime uses the existing runtime-handle
  release. ARC is unchanged.
- `bni` already links `bn_rt` as a Rust library (`bn_interpret_driver`,
  `bn_interp`, `bn_lib_sqlite` depend on it), so the approved single `unsafe`
  module `crates/bn_rt/src/ffi.rs` serves both backends.
- Crates (crates.io, 2026-10-07): `libffi` 5.2.0, `libffi-sys` 4.2.2,
  `libloading` 0.9.0. `libffi-sys` default builds bundled C: on MSVC with
  `build/msvc.rs` (x86, x86_64, aarch64); elsewhere with the shipped
  `configure` + `make` (needs `sh`, `make`, a C compiler; no autotools). The
  release matrix builds `x86_64-apple-darwin` on an arm64 runner: a cross
  `configure` (`--host`) to verify in S1.
- `bn_rt` already compiles bundled C (`rusqlite` `bundled`) on all three CI
  platforms.

### 1.3 Constraints

| Constraint | Consequence |
| --- | --- |
| `bn_llvm/src` budget (190 lines before 0.6.5a/b; measured after them) | one table-driven emission path: a table (member → ABI symbol, LLVM argument and result types) and one emitter; no per-type code. Measured at every gate; if it cannot fit, stop and ask Carlos |
| `unsafe` only in `crates/bn_rt/src/ffi.rs` (AGENTS.md, approved 2026-10-06) | `libloading::Library::new`, `Library::get` and `libffi` `Cif::call` live there, each under `#[allow(unsafe_code)]` with `SAFETY:` naming the checks that precede it |
| Forbidden-deps gate (`scripts/check-forbidden-deps.sh`) | `bnc` must not link the interpreter; `bni --no-default-features` stays language + HOST: `lib-ffi` is a default feature of `bni`, off without defaults |
| W4 | `IMPORT BNFfi` with `--target wasm32` is a support diagnostic, distinct from language errors |

### 1.4 Design (closed)

- **Runtime records.** `bn_rt::ffi` keeps a handle table of C values: each
  record holds its type code (a fixed number per `C.*` class, the technical
  decision in the proposal) and its payload. Interpreter objects map to these
  handles through the provider; native objects are the handles.
- **Library registry.** A process-wide registry (`Mutex<BTreeMap<String,
  Library>>`) loads each library once and never unloads it. It also caches
  each resolved `(library, symbol)` code pointer, so a repeated `C.Call`
  does not repeat the file search or `dlsym` (safe because nothing is
  unloaded).
- **No lock across the call.** The registry and record-table locks are
  released before `Cif::call`: a slow C function must not block other
  `BNDispatch` workers, and a C function that re-enters `bn_rt` must not
  deadlock.
- **Resolution order** (D4): a name with `/` or `\` is a path; otherwise each
  `--ffi-path` directory (`bni run` only), each `BN_LIBRARY_PATH` directory
  (through `bn_host_env::get`), then the system loader; platform patterns
  `lib<name>.so`, `lib<name>.dylib`, `<name>.dll`.
  - A name containing `.` is a file name and is not expanded
    (`"libm.so.6"`). Needed on Linux: with glibc, `libm.so` and `libc.so` are
    linker scripts (and exist only with the dev package), so `"m"` →
    `libm.so` fails in `dlopen`.
  - Directory candidates are joined into a full path before loading, so the
    loader does no search of its own for them.
  - An empty `BN_LIBRARY_PATH` entry is ignored; it never means the current
    directory (the `LD_LIBRARY_PATH` `::` pitfall).
  - Windows system-loader step: `LoadLibraryExW` with
    `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` (application directory, `System32`,
    added directories; not the current directory, not `PATH`), through
    `libloading::os::windows::Library::load_with_flags`. TODO: confirm the
    API in `libloading` 0.9.0 in S1.a. macOS: TODO confirm in S1.b whether
    `dlopen` of a bare name searches the current directory.
- **Loading runs code.** `dlopen` / `LoadLibrary` execute the library's
  initialisers. A library is loaded only by `C.Call` at run time; `bni check`,
  `bni lsp`, `bni dap` before launch and `bnc` never load one.
- **Call.** Build the libffi `Cif` from the argument codes and the result
  code; `C.String` → a `CString` alive for the call; an array → the region's
  data pointer, the region kept alive for the call; the result written into
  the result record. A `C.Bool` result is read as `uint8_t` and normalised to
  `value != 0`. Every check of the proposal's § Errors runs before the call.

---

## 2. Decisions

| ID | Decision | Owner |
| --- | --- | --- |
| **D1** | **Decided 2026-10-06 (Carlos): accepted.** `proposals/bnffi.md` supersedes `c-ffi.md` and AQ-22 (`native-stdlib-binding.md`); both carry the note | Carlos |
| **D2** | **Decided 2026-10-06 (Carlos): approved.** `libffi`/`libffi-sys` and `libloading`, and a narrow `unsafe` in `crates/bn_rt/src/ffi.rs` only, recorded in `AGENTS.md`. Windows MSVC build verified in S1.b | Carlos |
| **D3** | **Decided 2026-10-06 (Carlos): no policy.** `BNFfi` is always enabled; no environment variable, flag or profile denies `C.Call`; there is no `C.POLICY_DENIED`. `--sandbox` does not confine C code, and the documents say so. Recorded divergence: `host-traits.md` sets default-deny as the direction for powerful surfaces | Carlos |
| **D4** | **Decided 2026-10-06 (Carlos): search path given at run time, never recorded in the binary.** `bni run --ffi-path <dir>` (repeatable) and `BN_LIBRARY_PATH`; a compiled program takes `BN_LIBRARY_PATH` only. Search order: `--ffi-path`, `BN_LIBRARY_PATH`, the system loader. Whoever controls the environment can substitute a library of the same name (as with `LD_LIBRARY_PATH`); the documents say so | Carlos |
| **D5** | **Decided 2026-10-06 (Carlos):** error codes `C.INVALID_ARGUMENT` 1 … `C.INVALID_UTF8` 8 (proposal § Errors) | Carlos |
| **D6** | **Decided 2026-10-07 (Carlos): accessor methods instead of fields** (§1.2): `r.Value()` on every `C.*` value class and `C.HandleResult`. Methods are the only form native standard modules support today | Carlos |
| **D7** | TODO (Carlos): placement under the core/adapter rule of 0.6.5c. Recommended: the FFI logic and its `unsafe` live in a core `crates/bn_core_ffi` (feature `ffi`); `bn_lib_ffi` and `bn_rt/src/ffi_abi.rs` are adapters. This moves the approved `unsafe` location from `crates/bn_rt/src/ffi.rs` to `crates/bn_core_ffi/src/lib.rs`: the `AGENTS.md` approval must be restated with the new path. Alternative: keep `bn_rt/src/ffi.rs` as approved and list FFI in the 0.6.5c migration table. Blocks S3 | Carlos |
| **D8** | TODO (Carlos): type compatibility between C and BN (open question raised by Carlos). Recommended: the `C.*` classes *are* the compatibility layer: each constructor takes the BN type of the same range, and the C types BN lacks (`long`, `size_t`) map to the widest BN type with a range check (`VALUE_OUT_OF_RANGE`). Language-level C types are `EXTERN`, rejected by D1; the static mapping belongs to `proposals/c-export.md` | Carlos |
| **D9** | TODO (Carlos): `--sandbox` with `IMPORT BNFfi`. D3 leaves C unconfined and documented; a user who runs `bni run --sandbox` still believes the program is confined. Recommended: `bni run --sandbox` on a program that imports `BNFfi` fails before running, with a stable code, instead of running unconfined. This is not a policy on `C.Call` (D3 stands): the sandbox refuses a promise it cannot keep | Carlos |
| **D10** | TODO (Carlos): fewer scalar classes. On every supported target (Linux, macOS, Windows; x86_64, aarch64) `signed char`, `unsigned char`, `short`, `unsigned short`, `int`, `unsigned int`, `long long` and `unsigned long long` have the width of `int8_t` … `uint64_t`. Recommended: keep `C.Int8` … `C.UInt64`, `C.Int`, `C.UInt` (the most common C type, kept for readability), and only the platform-varying `C.Long`, `C.ULong`, `C.SizeT`; drop `C.SChar`, `C.UChar`, `C.Short`, `C.UShort`, `C.LongLong`, `C.ULongLong` (6 classes fewer in the module, provider, table and fixtures) | Carlos |
| **D11** | TODO (Carlos): the array classes. The proposal says "one class per numeric element type" and names only `C.DoubleArray`. Recommended: one per BN region element type: `C.ByteArray`, `C.Int8Array`, `C.Int16Array`, `C.Int32Array`, `C.Int64Array`, `C.UInt16Array`, `C.UInt32Array`, `C.UInt64Array`, `C.FloatArray`, `C.DoubleArray`. Must be fixed before S2.a | Carlos |

---

## 3. Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| **AC1** | The language did not change: `git diff` on `0.6.ebnf`, `0.6.md`, `keywords.md` is empty for this bucket | S2.d |
| **AC2** | The proposal's conformance program passes `bni check` with no workaround | S2.d |
| **AC3** | For every `C.*` scalar type, a call to the test library returns the expected value on both backends, exact output | S6 fixtures, `native_and_interpreter_print` |
| **AC4** | `C.String`, `C.DoubleArray` (C writes into the region), `C.HandleResult` (non-null and C `NULL`), `ReadFrom` / `WriteTo` / `ReadString` behave as the contract says, on both backends | S6 fixtures |
| **AC5** | Each error code 1–8 arises from its condition with the contract's `Code`, `Operation` and `Message`, on both backends | S6 error fixtures |
| **AC6** | `NEW C.Handle()` is `PRIVATE_ACCESS` | negative fixture |
| **AC7** | Resolution order D4 holds: under `bni run`, a library in `--ffi-path` wins over one in `BN_LIBRARY_PATH`; a compiled program, which has no `--ffi-path`, resolves through `BN_LIBRARY_PATH` then the system loader; an empty `BN_LIBRARY_PATH` entry does not load from the current directory; a dotted name is not expanded | S6.e |
| **AC8** | `wasm32`: support diagnostic, not a language error | S6.f |
| **AC9** | `unsafe` appears only in `crates/bn_rt/src/ffi.rs`, each use with `SAFETY:` | `rg` recorded (S3.h) |
| **AC10** | One implementation: resolution, checks and the call exist only in `bn_rt::ffi`; the provider and the emission only translate | `rg` recorded (S5.d) |
| **AC11** | Build and tests green on Linux, macOS, Windows (test host and CI), including the release matrix targets | logs |
| **AC12** | No library is loaded outside `C.Call` at run time: `bni check` and `bni lsp` on a program that calls a library whose initialiser writes a marker file leave no marker | S6.i |
| **AC13** | The per-call cost is measured, not assumed: 1 000 000 calls of `cos` through a wrapper class under `bnc` and `bni`, time recorded here with the machine | S6.h |

---

## 4. Sprints

**Every gate:** as in 0.6.5a §4 (fmt, clippy, build `bni bnc bn_rt`, named
tests, `git diff --check`, `bn_llvm` line count).

### S0 — Gate

- [ ] **S0.a** 0.6.5a, 0.6.5b and 0.6.5c closed (in `done/`).
- [ ] **S0.b** D7, D9, D10 and D11 decided and recorded above; D8 answered.

### S1 — Dependencies (spike, kept only if every platform builds)

- [ ] **S1.a** `bn_rt/Cargo.toml`: `libffi = "5.2.0"`, `libloading =
  "0.9.0"` behind feature `ffi` (default on); `Cargo.lock` updated;
  `cargo build -p bn_rt` on macOS.
- [ ] **S1.b** Build on the Windows test host (MSVC) and in CI on Linux
  (x86_64, aarch64) and macOS (aarch64, and `x86_64-apple-darwin` cross from
  the release job). *Done when* every target links a trivial `ffi` call;
  otherwise stop and report.
- [ ] **S1.c** `bni`: feature `lib-ffi` (default) through
  `bn_interpret_driver`; `cargo build -p bni --no-default-features` does not
  pull `libffi`; `scripts/check-forbidden-deps.sh` green.
- [ ] **S1.d** Native link: compare `cargo rustc -p bn_rt -- --print
  native-static-libs` with `native_runtime_link_args`
  (`bn_compile_driver/src/artifact.rs`, Linux passes only `-lm`). Add `-ldl`
  if the oldest supported glibc needs it (below 2.34, `dlopen` is in
  `libdl`). A program that does not import `BNFfi` links as before.

### S2 — Contract and module (no language change)

- [ ] **S2.a** `language/0.6/bnffi.md` from the proposal (surface with D6,
  errors, resolution, memory rules, the no-confinement statement).
- [ ] **S2.b** `modules/bn/BNFfi.bn` (stub bodies, `PRIVATE` constructor on
  `Handle`); `StandardModule::BNFfi`; `Module.bnffi_providers`.
- [ ] **S2.c** `docs/architecture/support-matrix.md` (Wasm row),
  `host-traits.md` (no policy; not confined by `--sandbox`).
- [ ] **S2.d** AC1 and AC2 evidence recorded here.

### S3 — `bn_rt::ffi` (the one implementation)

- [ ] **S3.a** Record table and type codes (one per `C.*` class).
- [ ] **S3.b** Library registry and resolution (§1.4), with `--ffi-path`
  passed in by the caller and `BN_LIBRARY_PATH` through `bn_host_env::get`;
  dotted names, empty entries, full-path loading of directory candidates,
  Windows search flags; `(library, symbol)` cache; no lock held across the
  call.
- [ ] **S3.c** Symbol lookup; `C.INVALID_ARGUMENT`, `LIBRARY_NOT_FOUND`
  (message lists every location tried), `LIBRARY_LOAD_FAILED` (loader text in
  `Cause`), `SYMBOL_NOT_FOUND`.
- [ ] **S3.d** Argument and result checks: `INVALID_SIGNATURE`,
  `VALUE_OUT_OF_RANGE` (target C width, e.g. `long` on Windows),
  `INVALID_STRING`.
- [ ] **S3.e** The call: `Cif` from codes; `CString` and region pointers kept
  alive for the call; result written to the result record; `HandleResult`
  null → BN `NULL`.
- [ ] **S3.f** `ReadFrom` / `WriteTo` (`LEN(region)` elements) and
  `ReadString` (`INVALID_UTF8`).
- [ ] **S3.g** Unit tests against libc symbols present on all three
  platforms (`labs`, `strlen`), each error path.
- [ ] **S3.h** AC9 `rg` recorded.

### S4 — Interpreter provider

- [ ] **S4.a** `crates/bn_lib_ffi` (`FfiProvider`): `CONSTRUCTOR` with
  arguments creates a record; `Value()`, `Args.Add`, `Args.Count`, `Call`,
  array and handle members delegate to `bn_rt::ffi`; `object_destroyed`
  frees the record. Registered in `libraries.rs` under `lib-ffi`.
- [ ] **S4.b** `bni run --ffi-path <dir>` (repeatable) in `bn_cli`
  options, passed to the provider; `--help` text.
- [ ] **S4.c** *Done when* the S6 fixtures pass under `bni`.

### S5 — Native backend

- [ ] **S5.a** `bn_rt/src/ffi_abi.rs`: `bn_rt_ffi_*` C ABI over S3 (no logic
  of its own).
- [ ] **S5.b** `bn_llvm`: one table (member → symbol, argument and result
  LLVM types) and one emitter for every `BNFfi` member; within budget.
- [ ] **S5.c** Symbols added to `value-memory-abi.md`
  (`tests/compiler_capabilities.rs` green).
- [ ] **S5.d** AC10 `rg` recorded. *Done when* the S6 fixtures pass under
  `bnc`.

### S6 — Fixtures (written before S3–S5 code; listed here by topic)

- [ ] **S6.a** `tests/ffi/bnffi_test.c`: one function per `C.*` type, an
  in-place array function, a function returning a heap pointer and its
  release function, one returning `NULL`, one returning a static string. The
  test harness builds it with `clang` into a temporary directory
  (`.so` / `.dylib` / `.dll`) and sets `BN_LIBRARY_PATH`.
- [ ] **S6.b** Value fixtures (AC3, AC4), exact expected output, both
  backends.
- [ ] **S6.c** Error fixtures (AC5), one per code.
- [ ] **S6.d** `NEW C.Handle()` negative fixture (AC6).
- [ ] **S6.e** Resolution order (AC7): the same library name in two
  directories with different results.
- [ ] **S6.f** `wasm32` support diagnostic (AC8).
- [ ] **S6.g** System library smoke: `cos` from `"libm.so.6"` on Linux,
  `"m"` on macOS, `"ucrtbase"` on Windows (system library names are not
  portable; the book shows the wrapper choosing the name).
- [ ] **S6.h** AC13 measurement. Optimisations beyond the symbol cache
  (reusable `C.Args`, value setters, a bound-function object) are decided on
  these numbers, in a later bucket.
- [ ] **S6.i** AC12: a test library with an initialiser
  (`__attribute__((constructor))`, `DllMain` on Windows) that writes a marker
  file; `bni check` and an LSP open of the program leave no marker; `bni run`
  does.

### S7 — Close

- [ ] **S7.a** `man/bni.1` (`--ffi-path`), `BN_LIBRARY_PATH` in `man/bni.1`
  and `man/bnc.1`; `help_and_man_page_document_the_same_options` green.
- [ ] **S7.b** Book chapter for `BNFfi` (`docs/book/en/`), including the
  no-confinement warning and the wrapper pattern; `toc.md`.
- [ ] **S7.c** `README.md`; `bn/basicnext-vscode`, `bn/basicnext-jupyter`
  (module name, completion).
- [ ] **S7.d** Windows test host; `scripts/test-battery.sh`; CI green (AC11).
- [ ] **S7.e** `docs/releases/0.6.6.md`, `docs/releases/README.md`; `git mv`
  the proposal to `done/proposals/` and this bucket to `done/`.

---

## 5. Risks

| Risk | Mitigation |
| --- | --- |
| `libffi-sys` cross build for `x86_64-apple-darwin` fails | S1.b is a spike: the bucket stops there and reports before any other work |
| `bn_llvm` budget | table-driven emission; measured at every gate; stop and ask if exceeded |
| A wrong C declaration crashes the process | out of the runtime's reach by design; the contract and the book say so; the wrapper pattern keeps declarations in one place |
| Building the test library on Windows needs the MSVC linker | the test host has VS 2022; CI's Windows job installs LLVM 22 |
| Logical names fail on Linux (`libm.so` is a glibc linker script) | dotted names are not expanded (§1.4); S6.g uses `"libm.so.6"` |
| Library substitution (DLL hijacking, `::` in a path variable) | full paths for directory candidates, empty entries ignored, Windows `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`; D4's remaining exposure (whoever sets `BN_LIBRARY_PATH`) documented |
| Opening a file in the editor runs C code | AC12: libraries load only in `C.Call` at run time |
| A long C call stalls other threads | no lock held across the call (§1.4) |
