# Basic Next 0.2 Delivery Bucket — Archived with Open Evidence

Archived on 2026-08-29 when the 0.3 bucket became active. Item R19 remained
open and was transferred without being marked complete to 0.3 gate `G0.1`.

This is the **0.2** bucket. 0.1 is frozen under `docs/language/0.1/`; its
completed sprint audits are archived under `done/project/`. The active 0.2
WBS is [`WBS-0.2.md`](../../ongoing/WBS-0.2.md).

Authority: [`0.2.ebnf`](../../docs/language/0.2/0.2.ebnf),
[`0.2.md`](../../docs/language/0.2/0.2.md),
[`keywords.md`](../../docs/language/0.2/keywords.md).

Open defects at archival time: [`gap_analysis.md`](../../ongoing/gap_analysis.md). Work program for
those defects: [0.2 remediation program](#02-remediation-program).


## 0.2 scope (accepted)

0.2 is **language OO + stdlib + positioned console + file I/O + Jupyter
kernel + `bn build`**.

Order: language/host on the interpreter, then the kernel, then LLVM.

Also in 0.2: string character access, `HOST.FileSystem` (class `File`, not
keywords), CSV read/write, `BNData.DataFrame`, class inheritance
(`EXTENDS` / `SUPER`), and interface polymorphism through virtual dispatch.

Not in 0.2: packages, `HOST.Network`, FFI, TZDB conversion, `PARALLEL`, JVM
backend, `MATCH`/`ENUM`/generics, LSP, package registry, a persistent
notebook REPL, `OPEN`/`CLOSE`/`READ`/`WRITE` as keywords.

Delivery rule unchanged: lexer → parser/AST → semantics → typed BN IR →
interpreter. Compilers consume **BN IR only**. `unsafe` remains forbidden
unless Carlos approves a narrow use.

```text
.bn source → lexer → parser/AST → semantics → typed BN IR → interpreter
typed BN IR → LLVM IR → { Windows | macOS | Linux | wasm32 }
```

## Sprint exit criteria

A sprint is complete only when every item in its section is checked and the
following evidence exists:

- Positive and negative conformance fixtures cover its accepted surface.
- `bn check` accepts and rejects the relevant fixtures with source-spanned
  diagnostics; `bn run` covers each executable behavior.
- Rust changes pass `cargo fmt --check`, `cargo test`,
  `cargo clippy -- -D warnings`, and `git diff --check`.
- Withdrawn forms have a negative fixture in the sprint that withdraws them.

Do not start the next sprint while its predecessor has unchecked work, except
for documentation-only preparation that does not change language behavior.

## 0.2 remediation program

This program records defects found by the full 0.2 implementation audit
([`gap_analysis.md`](../../ongoing/gap_analysis.md), 2026-08-28). A checked historical
sprint item is reopened when its normative behaviour is not implemented or
its evidence is insufficient. The gap file is the inventory; this section
is the work program.

Shared quality gate for every item: positive and negative fixtures with
source-spanned diagnostics; `bn check` / `bn run` (and `bn build` when the
item is a compiler path) cover the behaviour; Rust changes pass
`cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and
`git diff --check`. Do not mark an item complete while any of its **Done
when** bullets is open.

The historical attack order is R1 → R19. **U1 preempts R19 and release
closure.** Do not start compiler expansion (R11–R12) until R10 makes the
already-accepted LLVM subset match `bn run`. Do not start documentation-only
R16 while a P0/P1 item that it would describe is still wrong. After U1,
Windows real-TTY evidence is **R19** (last); it does not block R7 or later
interpreter work.

### U1 — Cross-artifact conformance and repository closure (urgent)

**Status: U1.1–U1.11 closed.** Cross-artifact lexical, editor, and repository
gates are enforced by `tests/u1.rs` and `plugins/vscode/test/grammar.js`.
U1.12 is the recorded exclusion: Windows TTY evidence stays **R19**.

#### Divergences

| ID | Status | Evidence |
| --- | --- | --- |
| U1.1 | Resolved | `IF` is in `0.2.ebnf` `reserved-word`. `build.rs` and `tests/u1.rs` require registry = EBNF. |
| U1.2 | Resolved | Marked `special-float-literals` list; generated `SPECIAL_FLOAT_LITERALS`; lexer uses it; `-INF` remains `Minus` + `INF`. |
| U1.3 | Resolved | `src/keyword_registry.rs` rejects empty/duplicate/reversed markers, `123`, lowercase, punctuation, unsorted lists, and EBNF drift. Runtime does not read `docs/`. |
| U1.4 | Resolved | `tests/u1.rs` feeds every generated reserved word and special literal through `lex`. |
| U1.5 | Resolved | Both TextMate copies include `PARALLEL` / `SYSTEM`; constants are `NAN`/`INF` without `-INF`; `grammar.js` compares the word-terminal union. |
| U1.6 | Resolved | Decimal regex has no exponent; string escapes are `\"` and `\\` only; `grammar.js` exercises the regexes. |
| U1.7 | Resolved | `keywords.md` records `ASC` / `CHAR` as identifiers. Fixture README uses the 0.2 identifier model for `CLS` / `BEEP`. |
| U1.8 | Resolved | Layout and contributing paths use `todo/proposals/` and `done/proposals/`. Link integrity is tested. 0.1 stays frozen. |
| U1.9 | Resolved | Sprint 4 audit lives in `ongoing/audit-sprint-4.md` (Windows = R19). Experience contract restored to `docs/project/experience-contract.md`. Sprint 10 evidence includes `grammar.js`. |
| U1.10 | Resolved | BNData, File I/O, and alternative-types archives are under `done/proposals/`. Mixed numeric-semantics stays in `todo/` with remaining proposed scope. |
| U1.11 | Resolved (0.2) | Language authority is 0.2: `docs/language/0.2/0.2.ebnf`, `0.2.md`, `0.2/keywords.md`. `AGENTS.md` is gitignored and now names that 0.2 registry. 0.1 stays frozen under `docs/language/0.1/`. |
| U1.12 | Excluded (R19) | Real Windows TTY capture remains R19. Not a unit test. |

#### Additional mandatory tests

1. **Normative lexical parity:** parse the marked lists in
   `docs/language/0.2/keywords.md` and the `reserved-word` /
   `special-float-literal` productions in `0.2.ebnf`; assert exact set
   equality, no duplicates, and no missing grammar terminal such as `IF`.
2. **Generated-registry validation:** exercise empty sections, duplicate or
   reversed markers, duplicate/out-of-order entries, lowercase text, invalid
   initial digits, punctuation, and a valid list. Each invalid registry must
   fail generation with an actionable message.
3. **Exhaustive lexer classification:** feed every generated reserved word to
   `lex` and require `TokenKind::Keyword`; feed every generated special literal
   and require `TokenKind::Special`; lowercase and mixed-case variants must be
   identifiers.
4. **Non-reserved boundary:** require `ASC`, `CHAR`, `CLS`, `BEEP`, `PRINTAT`,
   `OPEN`, `CLOSE`, `READ`, `WRITE`, `MATCH`, `OK`, `ERR`, and `Error` to
   remain legal identifiers. Require `PARALLEL`, `SYSTEM`, `NAN`, and `INF` to
   be illegal as declaration names for their respective lexical reasons.
5. **TextMate parity:** load both JSON grammars, assert they are equivalent,
   extract the union of word terminals, and compare it with generated keywords
   plus special literals. Explicitly assert scopes for `PARALLEL`, `SYSTEM`,
   `NAN`, and `INF`, and assert that `-INF` is not modeled as a separate
   lexical terminal.
6. **TextMate lexical boundaries:** positive/negative cases for decimal,
   binary, hexadecimal, forbidden exponent notation, valid escapes, invalid
   escapes, comments, and case sensitivity. The test must exercise regex
   behaviour, not merely compare JSON files.
7. **Documentation link integrity:** check every repository-relative Markdown
   link in active docs, `ongoing/`, `done/`, and `todo/`; reject missing targets
   while allowing external URLs and explicitly registered frozen exceptions.
8. **Workflow-location integrity:** reject `Status: Open`, unchecked completion
   gates, or unresolved `TODO:` markers beneath `done/`; flag documents marked
   accepted beneath `todo/` unless they explicitly retain unresolved scope.
9. **Package-input integrity:** `cargo package --list` must contain `build.rs`
   and `docs/language/0.2/keywords.md`, and must not depend on the removed
   `docs/language/keywords.md` path.
10. **Real-host evidence:** retain the R19 PowerShell/TTY procedure. Do not
    replace Windows console evidence with mocks or a non-Windows simulation.

#### U1 completion gate

- [x] U1.1–U1.12 are resolved or explicitly excluded by a recorded BDFL
      decision; no silent exception. U1.12 is R19.
- [x] The parity tests fail when one keyword or special literal is deliberately
      removed from either normative source.
- [x] The lexer and TextMate boundary tests fail on a deliberate category or
      regex regression.
- [x] Active Markdown links resolve and directory status rules pass.
- [x] Sprint 0, R16, and Sprint 10 audits are reconciled only after their new
      evidence exists.
- [x] `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`,
      Python kernel/compiler parity tests, both Node extension tests,
      `mandoc -Tlint docs/man/bn.1`, workflow YAML parsing,
      `cargo package --list --allow-dirty`, and `git diff --check` pass.
      Wasm32 uses a Clang with a `wasm32` target (Homebrew LLVM 22), not
      Apple Clang; `wasm-ld` from Homebrew `lld@20`.
- [x] R19 remains visibly open until real Windows TTY traces are committed.

### Historical P0/P1/P2/P3 (pre-gap)

Kept as the previous remediation record. Unchecked lines below are still
open work and are restated with criteria in R1–R19.

- [x] Kernel execution invokes `bn run --no-filesystem`. Denial keys
      off the executable `IMPORT HOST.FileSystem` (R8).
- [x] Validate inbound Jupyter signatures; stdin is ROUTER.
- [x] Cover the Jupyter wire protocol with a non-skipped integration test.
      **Reopened by R1:** the test never sends `kernel_info_request`.
- [x] Standard Jupyter `input_request` / `input_reply`; legacy
      `execute_request.content.stdin` removed on the ZMQ path.
- [x] User imports below `modules/`; language modules below `modules/bn/`.
- [x] Logical single-segment `BN*` without a hard-coded module-name list.
- [x] `BNData` stdlib-provider keyed to the imported standard module.
- [x] `BNMath.MIN` / `MAX` empty-vector and `NAN`/`NA`/`FLOAT32` rules.
- [x] Unseeded `HOST.Random` has a non-zero host-selected state
      (interpreter only; compiler is R10).
- [ ] Real-TTY `NumCols` / `NumRows` on Windows. See R19. Unix ioctl
      and the stdin-pipe PTY fixture are R7.
- [x] Filesystem text/binary commit after success. Closed by R4 / R8.
- [x] Negative CSV / DataFrame bounds tests. Closed by R4 / R6.
- [x] Typed BN IR lowering (not pattern matching). See R11.
- [x] Derive target capabilities from typed BN IR.
- [x] Link and execute WebAssembly. See R12.
- [x] Native and WASM parity on the accepted non-TTY surface. See R10–R12.
- [x] CI on the documented target matrix. Closed by R15.
- [x] VS Code diagnostic parsing, save policy, missing-tool reporting, and
      launch-only DAP lifetime. Closed by R17.
- [x] Reconcile sprint records, README, manual, WBS, and bucket. Closed by
      R16. Per the BDFL, the maintainer-local `AGENTS.md` worktree state is
      outside release closure and was not changed.

### R1 — Jupyter `kernel_info` handshake

Reopens Sprint 7. Gap #1.

- [x] **Objective:** a stock JupyterLab / `jupyter_client` handshake
      succeeds against `bn-kernel`.
- **Fix:** handle `kernel_info_request` on shell (and control if sent
      there). Reply `kernel_info_reply` with Jupyter protocol 5.x,
      `implementation` / `implementation_version`, and `language_info`
      (`name`: `basicnext`, `file_extension`: `.bn`, `mimetype` text).
      Ignore unknown requests without dropping the session.
- **Tasks:**
  1. Implement `kernel_info_reply` in `plugins/jupyter/bn_kernel/jupyter.py`.
  2. Extend `tests/test_jupyter.py` to send `kernel_info_request` first
     and assert the reply fields.
  3. Update `docs/project/kernel.md` to the actual protocol (no legacy
     `execute_request.content.stdin`).
- **Done when:** `wait_for_ready()`-style `kernel_info` round-trip passes
  in `tests/test_jupyter.py`; JupyterLab can select the `bn` kernel
  without hanging on startup (manual note in the sprint 7 audit).

### R2 — Kernel pipes, heartbeat, interrupt, shutdown

Reopens Sprint 7. Gaps #2, #36, #37.

- [x] **Objective:** a cell cannot deadlock the kernel; heartbeat stays
      alive during execute and `INPUT()`; shutdown and interrupt are
      real protocol messages.
- **Fix:** never drain stderr to EOF before reading stdout. Read both
      pipes concurrently (threads or `select`). Serve heartbeat on its
      own socket/thread so `_execute` / `_input_reply` cannot starve it.
      `interrupt_request` kills the `bn` child. `shutdown_request` is
      tested on the wire. Bound `_input_reply`: if the child exits after
      `BN_INPUT_REQUEST`, return from the wait.
- **Tasks:**
  1. Replace the stderr-then-stdout loop in `jupyter.py` `_execute`.
  2. Keep heartbeat responsive during execute and stdin wait.
  3. Implement `interrupt_request` (SIGTERM/terminate the child).
  4. Fixture: `PRINT` of >64 KiB with no `INPUT()` completes.
  5. Fixture: `INPUT()` then a cancelled/abort reply still ends the cell.
  6. Fixture: `shutdown_request` receives `shutdown_reply` (do not only
     `process.terminate()` in the test).
- **Done when:** the large-`PRINT` cell returns; heartbeat is answered
  during a multi-second cell; interrupt stops `bn`; the shutdown test
  sends `shutdown_request`. JSON-lines `kernel.py` stays on
  `communicate` (already safe).

### R3 — `NEW` / `SUPER` construction order

Reopens Sprint 1. Gap #3, #13 in gap (pin of `$fields`).

- [x] **Objective:** `NEW Derived(...)` matches `0.2.md` Construction.
- **Fix:** allocate the most-derived object; run `SUPER` (explicit or
  implicit) which runs **base field initializers then the base
  constructor**, recursively; then derived field initializers; then the
  rest of the derived constructor. Pin dispatch for `$fields` the same
  way constructors/destructors are pinned, so a method call from a
  field initializer uses the class whose initializers are running.
- **Tasks:**
  1. Change IR lowering so `Derived.$fields` is **not** run before
     `SUPER`. Base `$fields` belongs to the `SUPER` step.
  2. Positive fixture: subclass field `PUBLIC label AS STRING = SELF.name`
     sees the value assigned in the base constructor, not `""`.
  3. Positive fixture: a method called from a base field initializer is
     the base implementation, not a subclass override.
  4. Keep existing inheritance fixtures green (implicit `SUPER()`,
     explicit `SUPER(args)`, destructor chain most-derived first).
  5. Negative: `SUPER` in a destructor is a static error (gap #20).
- **Done when:** `bn check` / `bn run` fixtures cover steps 1–4 of
  `0.2.md` Construction and Destruction; destructor `SUPER` is rejected
  with a source span; dispatch pin during field init is tested.

### R4 — File and CSV return `T OR Error`

Reopens Sprint 5 and Sprint 6. Gaps #4, #5, #6.

- [x] **Objective:** file and CSV operations never raise a runtime
      diagnostic when the contract says `T OR Error`.
- **Fix:** treat `Value::Error` from `Write`/`WriteLine` as the
  `WriteCSV` result (stop the loop). Unterminated quotes return
  `Error`, same as ragged rows. `WriteBytes` I/O failure returns
  `Error`, matching text `Write`. `ReadBytes` on a closed file or
  wrong family: keep `Error` only if the declared type includes it;
  otherwise pick one channel and document it in `host.md` — do not
  abort with `IO_ERROR` while closed-file already returns `Error`.
- **Tasks:**
  1. `WriteCSV`: if any `WriteLine` yields `Error`, return that `Error`.
  2. Fixture: write to a closed file → `result IS Error`, process exit 0.
  3. Unterminated quoted CSV field → `Error`, not `CSV_ERROR` diagnostic.
  4. `WriteBytes` disk/closed failure → `VOID OR Error`, exit 0.
  5. Align `ReadBytes` closed/text-mode/I/O with `host.md`; add fixtures.
- **Done when:** no file/CSV path in `src/runtime.rs` uses
  `runtime_error("IO_ERROR")` or `runtime_error("CSV_ERROR")` for a
  documented `T OR Error` operation; fixtures cover closed, quoted, and
  write-bytes failure.

### R5 — `IS` and equality for `File`, `DataFrame`, and `Error`

Gaps #7, #8, #29.

- [x] **Objective:** alternative types `FS.File OR Error` and
      `DataFrame OR Error` are testable with `IS`, and reference
      equality matches 0.1 object identity.
- **Fix:** `is_value` must recognise `Value::File` and
  `Value::DataFrame`. IR `IS` type names must not insert spaces
  (`FS.File`, not `FS . File`). `equals` compares `File`/`DataFrame`
  by handle id. `Error` equality is by `Code` and `Message` (or
  identity if the spec says object — follow `error.md` / 0.1; do not
  invent). Default `Error` must be a real `Error` value (`Code` /
  `Message` exist; `e IS Error` is true). `coerce` must accept
  `Value::Error` into `Type::Named("Error")`.
- **Tasks:**
  1. Fix IR type-name rendering for dotted names in `IS`.
  2. Runtime `is_value` / `equals` / `coerce` / `empty_named`.
  3. Fixtures: `Open` success → `IS FS.File`; failure → `IS Error`;
     two names bound to the same open file compare equal; default
     `LET e AS Error` is `IS Error` and has `.Code`.
- **Done when:** those fixtures pass under `bn check` and `bn run`;
  `VOID` default and `Error` default no longer compare equal.

### R6 — DataFrame bounds and reductions

Reopens Sprint 6. Gaps #9, #10, #28.

- [x] **Objective:** `Select` / `Slice` out-of-range is `Error`; empty
      and all-`NA` reductions follow `BNMath` / `bndata.md`.
- **Fix:** negative `Select` indices return `Error`, like positive OOB
  and like `Slice`. Empty numeric `Mean`/`Median`/`Quartile*`/`Range`
  yield `NAN`, not `Error`. All-`NA` integer columns follow the same
  `NA`/`NAN` rules as `BNMath`. `Slice` on a frame with zero columns
  still validates the requested row range.
- **Tasks:**
  1. `Select([-1], [0])` → `Error` (run, not abort).
  2. `NEW DataFrame()` then `Slice(0, 1, 0, 0)` → `Error`.
  3. Empty float column `Mean` → `NAN`; all-`NA` integer stats per
     `math.md`.
- **Done when:** fixtures exist for negative `Select`, empty-frame
  `Slice`, empty/`NA` reductions; no `INDEX_OUT_OF_BOUNDS` diagnostic
  on those library calls.

### R7 — Console size from the stdout window

Reopens Sprint 4. Gap #21. Windows real-TTY capture is **R19**.

- [x] **Objective:** `NumCols` / `NumRows` / `PrintAt` bounds use the
      current **stdout** window, as `console.md` states.
- **Fix:** stop using `stty size` on inherited stdin. Use ioctl
  (`TIOCGWINSZ`) on stdout on Unix. Win32 `STD_OUTPUT_HANDLE` is
  already the Windows query. Keep the `stdout().is_terminal()` guard.
- **Tasks:**
  1. Replace Unix `stty` with ioctl on the stdout fd.
     `TIOCGWINSZ` on fd 1 (Linux `0x5413`, Darwin/BSD `0x40087468`),
     same narrow `unsafe` pattern as the Win32 query.
  2. Fixture: stdout TTY + stdin pipe still returns the window size
     (not `HOST_CAPABILITY_UNAVAILABLE`).
     `tests/cli.rs` `console_size_uses_stdout_when_stdin_is_piped`
     + `tests/console_stdout_tty.py`. macOS PTY 80×24 → `8024`.
- **Done when:** Unix ioctl is in tree; a PTY with piped stdin does
  not fail the size call. Windows real-TTY resize / `PrintAt` OOB
  evidence is R19 and does not block this item.

### R8 — Filesystem capability, directories, EOF family, `Close`

Reopens Sprint 5 and the kernel `--no-filesystem` P0. Gaps #22–#25.

- [x] **Objective:** capability denial matches `host.md`; directory
      `Open` is `Error`; EOF does not desynchronise text/binary;
      explicit `Close` flushes and can return `Error`.
- **Fix:** `--no-filesystem` / `HostEnv::without_filesystem` fails
  **before `Start`** if the executable module **imports**
  `HOST.FileSystem`, even when no `FS` name is used in the body. Store
  that fact on the IR module or the host check, not a scan for
  `Constant::Type`. `Open` of a directory returns `Error`. `ReadLine`
  EOF on a never-used handle must not leave the other family callable
  if that contradicts “first successful family method” — pick the
  `host.md` reading and fixture both orders (`ReadLine` EOF then
  `ReadBytes`, and the reverse). Explicit `Close` flushes; flush/close
  failure is `Error`. Destructor close may still swallow errors.
- **Tasks:**
  1. Import-only `HOST.FileSystem` program is rejected by
     `bn run --no-filesystem` and by the kernel before `Start`.
  2. `Open(".")` → `Error`; fixture must `PRINT` both Open and
     `DeleteFile` results so a single success cannot hide a failure.
  3. EOF family fixtures for empty files.
  4. `Close` after a write: data is on disk; induced flush failure
     (if portable) returns `Error`.
- **Done when:** kernel import-only FS is denied; directory open is
  `Error` on Unix and Windows; EOF family fixtures pass; `Close`
  flush is in the implementation and documented.

### R9 — `bn check` rejects illegal programs

Gaps #11–#20, #45, #48.

- [x] **Objective:** programs that 0.2 forbids fail at `bn check` with
      a source-spanned diagnostic; programs that 0.2 allows parse.
- **Fix:**
  - `FOR EACH`: binding is read-only; iterable is a fixed-length
    vector; element type matches the declared type.
  - Postfix `AS`: static conversion rules from 0.1 (incorporated);
    `"hi" AS INTEGER` is a check error.
  - `CLS` / `BEEP` are not reserved words and not statements. They
    are ordinary identifiers. `CLS(HOST.Console)` is a call (and
    should fail as an unknown function unless the user declared
    `CLS`). Keep negative fixtures for the withdrawn **statements**.
  - Assignment is detected from the statement form, not “any `=` on
    the line”. `DoWork(flag = TRUE)` is a call using `=`.
  - Tokens after `CLASS`/`STRUCT`/`INTERFACE` name (other than
    `EXTENDS`/`IMPLEMENTS`) are a syntax error.
  - Lexer rejects `1.`; `.5` stays rejected.
  - User `T[]` (variable-length vector) is a static error. Keep
    `POINTER TO INTEGER[]` and `BNData` library signatures as the
    0.2 exception if `bndata.md` requires them — do not silently
    give users `INTEGER[]` locals.
  - Interface upcast requires the same imported interface, not the
    last name segment.
  - `EXTENDS Data.DataFrame` (and `FS.File`, `Error`) is a static
    error.
  - `SUPER` in a destructor is a static error (also R3).
  - Trailing comma in `FUNCTION` parameters is a syntax error.
  - `HOST.Args[i] = …` diagnoses “not an lvalue” / immutable, not
    “only LEN or index”.
- **Tasks:** add a negative fixture for each bullet, and a positive
  fixture for `DoWork(flag = TRUE)` if equality-as-argument is valid
  0.2 (it is `=` not `==`; confirm against EBNF — if equality in
  arguments is valid, parse as call).
- **Done when:** each case has a fixture; `bn check` is the failing
  command; no item is only a runtime error when the spec says static.

### R10 — Accepted LLVM subset matches `bn run`

Reopens Sprint 9 for correctness of what is already emitted. Gaps
#31–#34, #32.

- [x] **Objective:** if `bn build` accepts a program, the native
      artifact matches `bn run` on stdout and exit code for that
      program, including negative integers, overflow, and float print.
- **Fix:** in the subset that currently lowers:
  - integer `DIV`/`%` are Euclidean; divisor 0 is a diagnostic, not
    LLVM `sdiv` UB.
  - integer `+` `-` `*` overflow is `NUMERIC_OVERFLOW`, not wrap.
  - `SHR` matches the interpreter (logical shift of the width).
  - `/` is float; the constant folder must not `checked_div` integers.
  - every `PRINT` in a block is emitted; SSA names are unique.
  - float `PRINT` matches `render` (`1.0`, `NAN`, `INF`).
  - unseeded `HOST.Random` is not silently `1` unless that is also
    the interpreter host seed for that run — prefer requiring `Seed`
    for parity, or sharing the host seed rule.
- **Tasks:**
  1. Fixtures: `PRINT (-5) DIV 3` → `-2`; `(-5) % 3` → `1`;
     `PRINT 1 DIV 0` does not produce a crashing binary.
  2. `PRINT 1` then `PRINT 2` in one `Start` matches interpreter.
  3. Two `PRINT` of the same value produce valid LLVM (`llvm-as`).
  4. `PRINT 1.0` → `1.0\n`.
  5. Add those programs to `tests/test_compiler_parity.py`.
- **Done when:** parity tests include the cases above; no accepted
  program uses `sdiv`/`srem` or wrapping `add` for BN integers.

### R11 — Typed BN IR lowering

Existing P2 / Sprint 9. Gap #30.

- [x] **Objective:** LLVM emission walks typed BN IR instructions
      instead of recognising source-shaped patterns.
- **Fix:** one lowering path per IR instruction class that 0.2 `bn
      build` claims to support. Unsupported IR stays
      `BUILD_LOWERING_UNAVAILABLE` with the instruction name. Do not
      expand to objects/vectors/FS/console until R10 is done.
- **Tasks:** replace `constant_print` / `scalar_module` / template
  cascade with instruction lowering; keep the explicit unavailable
  diagnostic.
- **Done when:** adding a new supported instruction is a lowering
  arm, not a new pattern matcher; `BUILD_LOWERING_UNAVAILABLE` still
  fires for objects, vectors, FS, positioned console.

### R12 — Executable WASM and non-TTY parity

Existing P2 / Sprint 9. Gaps #35, #47.

- [x] **Objective:** `bn build --target wasm32 -o` produces a module
      that a documented WASM host can execute, matching `bn run` on
      the accepted non-TTY suite.
- **Fix:** stop treating `clang -c` object emission as the wasm32
  deliverable. Link a runnable module (WASI or documented host
  imports for `PRINT`/`INPUT`/`HOST.Random`). Run parity in that
  host. Native parity grows with the lowering, not 22 constant
  programs only.
- **Tasks:** document the WASM host; add a CI/local test that
  executes a wasm artifact; expand `test_compiler_parity.py` as
  lowering grows.
- **Done when:** a wasm32 artifact for `empty-start.bn` and the
  seeded-random fixture runs in the documented host and matches
  `bn run`; `compiler-0.2.md` no longer calls `-c` objects “the
  wasm32 target”.

### R13 — `BNMath` as a standard module

Gap #39.

- [x] **Objective:** `BNMath` follows the same rule as `BNData`:
      `modules/bn/BNMath.bn` is the semantic API; runtime dispatch is
      keyed to the imported standard module, not a hard-coded name
      table that ignores the file.
- **Fix:** put the accepted 0.2 signatures (functions and range
  constants) in `BNMath.bn`. Semantic analysis loads them from that
  module. Runtime/compiler intrinsics stay behind the provider, keyed
  by module id. Empty-comment `BNMath.bn` is not acceptable.
- **Tasks:** write the API in `BNMath.bn`; delete the special-case
  `IMPORT BNMath` type injection that does not read the file; keep
  existing math fixtures green.
- **Done when:** removing a function from `BNMath.bn` makes
  `Math.ThatName` a `bn check` error; `language-tour.bn` still runs.

### R14 — Clock, `Timestamp.Parse`, module identity

Gaps #26, #27, #44.

- [x] **Objective:** `TIMESTAMP` before 1970 is negative; RFC 3339
      fractions with 1+ digits parse; the same `.bn` file is one
      module.
- **Fix:** `duration_since` error on pre-epoch must not become `0`;
  use signed difference. `parse_hms` accepts 1–3+ fractional digits
  per `temporal.md`. `module_graph::normalize` canonicalizes paths
  (at least `canonicalize` / stable absolute) so `./a.bn` and `a.bn`
  are one `ModuleId`. `IMPORT BN*` must not pick a cwd `modules/bn`
  overlay unless that is the documented search rule — if it is not
  in `0.2.md`, do not keep the fallback.
- **Tasks:** fixtures for `Timestamp.Parse("2020-01-01T00:00:00.5Z")`;
  a test clock before epoch if injectable; two path spellings of the
  same import are one module.
- **Done when:** those fixtures pass; `normalize` is not identity.

### R15 — CI runs the quality gate

Existing P3. Gap #38.

- [x] **Objective:** a push to `main` cannot publish `latest` if fmt,
      tests, clippy, kernel, extension, or compiler parity failed.
- **Fix:** a test workflow (or extra jobs in `binaries.yml`) runs
  `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`,
  `python -m unittest tests/test_kernel.py tests/test_jupyter.py
  tests/test_compiler_parity.py`, and `node plugins/vscode/test/*.js`.
  Pin LLVM/clang for `bn build` jobs as `compiler-0.2.md` claims.
  Publish binaries only after those jobs pass.
- **Tasks:** add the workflow; stop `binaries.yml` from uploading
  `latest` on a red test job.
- **Done when:** a deliberate failing test would block publish; the
  workflow file is the evidence.

### R16 — Documentation matches the tree

Existing P3. Gaps #40–#42 and the P3 table.

- [x] **Objective:** a reader of `--help`, `bn.1`, `usage.md`,
      `kernel.md`, README, WBS, sprint audits, and examples sees 0.2
      as implemented, including `ongoing/`.
- **Fix / tasks:**
  1. `--help`: `bn build` emits LLVM for a subset; document
     `--no-filesystem` (and `--jupyter-stdin` as kernel-private or
     omit it from user help but document it in `kernel.md`).
  2. `docs/project/usage.md`: 0.2 CLI, `HOST.Args`, `bn build`.
  3. `kernel.md`: `input_request`; positioned console fails at call;
     FS denied on import (after R8).
  4. `examples/lexical.bn`: stop importing `HOST.Main`.
  5. `bn.1`: `docs/book/en/toc.md`.
  6. WBS: do not cite a missing `WBS-0.1.md`, or restore that file.
  7. Sprint 7 audit: Status Open until R1–R2 are done.
  8. Remove dead `HOST.Main.*` runtime/IR if nothing can reach it.
  9. Diagnostic strings that still say “0.1 capability”.
  10. `bn_kernel` “zero-dependency” vs `pyzmq`.
  11. Launcher `-f` / `--bn` must not `IndexError` when the flag is
      last without a value.
- **Done when:** grep for `HOST.Main` in `examples/` and `usage.md`
  is clean; `--help` matches `bn build`; sprint 7 audit no longer
  says complete while R1/R2 are open.
- **Completion evidence:** active documentation and archived sprint paths
  resolve against the current tree; `mandoc -Tlint docs/man/bn.1` passes;
  count overflow and heap reservation failures have checked diagnostics.

### R17 — VS Code remaining host issues

Gap #43. Sprint 10 lint/save-race stays done.

- [x] **Objective:** Run uses the buffer the user sees; missing `bn`
      does not clear diagnostics; DAP `terminated` means the process
      ended.
- **Fix:** save (or run from the unsaved buffer via a temp file)
  before `bn run`. On `execFile` `error` (ENOENT), publish a
  workspace diagnostic instead of `[]`. DAP: do not emit `terminated`
  on `runInTerminal` response; wait for the terminal process or stop
  claiming to be a debugger (document “launch only”).
- **Tasks:** Node tests for ENOENT and for run-after-dirty-buffer
  policy; debug-adapter test no longer treats “terminal opened” as
  program end unless that is the documented model.
- **Done when:** those tests pass; README of the extension states
  the debugger limits honestly.

### R18 — `POINTER TO` named-type / `VOID`

Gap #46. Authority conflict: `0.2.ebnf` allows `named-type` | `VOID`;
semantics reject non-numeric with a “0.1” message.

- [x] **Objective:** do not keep a silent split between grammar and
      checker.
- **Decision (BDFL, 2026-08-29):** keep `POINTER TO VOID` with C-style
  opaque-pointer semantics. Typed pointers convert to and from the opaque
  form when their region shapes are compatible. The opaque form cannot be
  indexed directly and `NEW VOID` remains invalid.
- **Fix:** report the conflict to the BDFL if 0.1 prose (numeric
  only) still applies via incorporation. Until that decision, do not
  “pick a side” in code. After the decision: either accept the EBNF
  forms with fixtures, or change the EBNF and keep the rejection with
  a 0.2 diagnostic.
- **Tasks:** a short note in this file or `0.2.md` recording the
  decision; then the matching fixture.
- **Done when:** grammar, keywords, and `bn check` agree; the
  diagnostic no longer says “0.1”.

### R19 — Windows real-TTY console evidence

Deferred from R7 so interpreter work is not blocked on a Windows
host. Gap #21 remainder; historical P0 “real-TTY Windows”.

- [ ] **Objective:** record real-TTY evidence on Windows for
      `NumCols` / `NumRows` / `PrintAt`, matching `console.md`.
- **Fix:** Win32 `STD_OUTPUT_HANDLE` is already the query. This item
  is capture, not a second implementation. Run on a Windows console
  (Windows Terminal or `conhost`): window size, resize, `PrintAt`
  in-bounds, `PrintAt` OOB → `INDEX_OUT_OF_BOUNDS`. Piped stdout
  still raises `HOST_CAPABILITY_UNAVAILABLE` at the call.
- **Tasks:**
  1. On a Windows TTY: `bn run tests/grammar/valid/console-size.bn`
     after resize; paste command output in this file or the sprint 4
     audit.
  2. `PrintAt` at `(1, 1)` and one coordinate past `NumCols` /
     `NumRows`.
  3. Confirm `echo | bn run` on a TTY still uses the **stdout**
     window (not stdin).
- **Done when:** those three traces exist in-tree; R7 stays the Unix
  ioctl + PTY fixture.

## Sprint 0 — Spec freeze

- [x] `docs/language/0.2/0.2.ebnf` (header 0.2, `named-type`, `SUPER`; no
      `CLS`/`BEEP`/`PRINTAT` statements). **Evidence reopened by U1.1–U1.4.**
- [x] `docs/language/0.2/0.2.md`.
- [x] `keywords.md` (`EXTENDS`, `SUPER`, console methods, no I/O keywords).
- [x] Library contracts: `math.md`, `host.md`, `console.md`, `bndata.md`.
- [x] `ongoing/WBS-0.2.md`.

Do not fold 0.2 features into `0.1.md`.

## Sprint 1 — Language OO

- [x] `EXTENDS` / `SUPER` / virtual override / parent upcast.
      **Reopened: R3** (construction order of `$fields` vs `SUPER`;
      destructor `SUPER`; `$fields` dispatch pin). **R9** (interface
      upcast by last name segment; `EXTENDS Data.DataFrame`).
- [x] Read-only `STRING` index `s[i]` (Unicode scalar, 0-based), including
      Unicode and out-of-bounds run fixtures.
- [x] Negative fixtures: `SUPER` as value, `SUPER` not first in constructor,
      assign to `s[i]`, private base members from the subclass.
- [x] `IMPLEMENTS Pets.Named` after `IMPORT Pets AS Pets` (qualified
      `named-type`).

## Sprint 1.1 — Command-line environment

This is the remaining mandatory 0.2 language surface omitted from the
original sprint list. It completes before Sprint 2.

- [x] `HOST.Args` is available only in the executable module, only through
      `LEN(HOST.Args)` and `HOST.Args[index]`, with immutable `STRING`
      entries and source-spanned errors for every other use.
      **R9:** `HOST.Args[i] = …` must diagnose an immutable lvalue, not
      “only LEN or index”.
- [x] `bn run file.bn -- args...` supplies the absolute executable entry at
      index `0` and the program arguments thereafter; negative and
      out-of-range indices raise `INDEX_OUT_OF_BOUNDS`.
- [x] Withdraw `HOST.Main`, `SYSTEM`, `ArgumentCount()`, and `Argument(index)`;
      add negative fixtures for each withdrawn form.

## Sprint 2 — BNMath 0.2

Canonical names stay the 0.1 IEEE surface. **No Portuguese aliases.**
Teaching maps `SEN→SIN`, `CON/COS→COS`, `ASN→ASIN`, `ACS→ACOS`, `LN→LOG`,
`SQR→SQRT`. Angles remain radians.

Already in 0.1: `ABS`, `SIN`, `COS`, `TAN`, `ASIN`, `ACOS`, `LOG`, `EXP`,
`SQRT`, plus `MIN`/`MAX`/`SIGN`/`FLOOR`/`CEIL`/`TRUNC`/`ROUND`/`HYPOT`/`FMA`/
`LOG10`/`LOG2`/`POW`/`ATAN`/`ATAN2`.

- [x] `BNMath.VAL(text AS STRING) AS FLOAT` (classic BASIC; see `math.md`).
      **R13:** `modules/bn/BNMath.bn` must be the semantic API, not an
      empty comment plus a hard-coded builtin table.
- [x] Range constants `MAX_INTEGER`, `MIN_INTEGER`, `MAX_FLOAT`, `MIN_FLOAT`,
      and width-specific names.
- [x] Descriptive statistics (`MEAN`, `MEDIAN`, `QUARTILE1`, `QUARTILE3`,
      `MODE`, `STDEV`, `VARIANCE`, `RANGE`; one-argument `MIN`/`MAX`).
- [x] Remove `Float.TryParse` from the 0.2 surface. Update
      `examples/rpn-calculator.bn`. `Date.Parse` / `Time.Parse` /
      `Timestamp.Parse` stay.

`RND` is **not** `BNMath`. Randomness is `HOST.Random`.

Fixtures: odd/even `n`, integers promoted to `FLOAT` for means, `MODE` tie
→ `NA`, `MIN` arity 1 vs 2, `VAL("3,14")` is `3.0`.

## Sprint 3 — `HOST.Random`

- [x] `IMPORT HOST.Random AS R`. Any module may import it (same rule as
      `HOST.Clock`, not `HOST.Main`).
- [x] `R.Random() AS FLOAT` in `[0, 1)`.
- [x] `R.Seed(n AS INTEGER) AS VOID`. Without `Seed`, the host chooses.
      Tests inject a provider, same idea as `HOST.Clock`.
      **R10:** compiled unseeded `Random` must not silently hard-code `1`
      while claiming parity with `bn run`.

## Sprint 4 — Console methods

`PRINT` / `INPUT()` stay stream macros. **No ncurses.** Windows Terminal,
macOS, and Linux TTYs speak VT. Piped stdout is not a window.

```basic
IMPORT HOST.Console AS CON
CON.Cls()
CON.Beep()
CON.PrintAt(column, row, text)
```

- [x] Withdraw `CLS(HOST.Console)` and `BEEP(HOST.Console)` as statements.
      **Reopened: R9** — lexer still reserves `CLS`/`BEEP`; parser still
      treats them as 0.1 statements.
- [x] `Cls()` / `Beep()` (piped OK).
- [x] `PrintAt(column, row, text)` — `INTEGER` 1-based; `(1, 1)` top-left;
      one `STRING`; no newline; OOB → `INDEX_OUT_OF_BOUNDS`; no wrap/clip.
- [x] `NumCols()` / `NumRows()` — current window, not cached at `Start`.
      **R7:** Unix ioctl on stdout; stdin-pipe PTY fixture. **R19:**
      Windows real-TTY evidence (resize / `PrintAt` OOB).
- [x] Non-TTY stdout → `HOST_CAPABILITY_UNAVAILABLE` **at the call** of
      `PrintAt` / `NumCols` / `NumRows`. `PRINT`/`INPUT()`/`Cls`/`Beep`
      still run when piped.
- [x] Cover imported aliases and direct `HOST.Console` calls, plus a
      non-executed TTY-only call in a false branch.

## Sprint 5 — `HOST.FileSystem`

```basic
IMPORT HOST.FileSystem AS FS
LET file AS FS.File OR Error = FS.Open(path, FS.READ)
```

- [x] Surface: `FS.File`, `Open`, `READ` / `WRITE` / `APPEND`, `Exists`, and
      `DeleteFile`, including `T OR Error` types and import-time capability
      failure. R5 / R8 closed the remaining contract holes.
- [x] Text and byte operations: `Close`, `ReadLine`, `ReadAll`, `ReadBytes`,
      `Write`, `WriteLine`, and `WriteBytes`. **Reopened: R4** (`WriteBytes`
      / `ReadBytes` I/O abort).
- [x] File state machine: closed-file errors, text/binary-family exclusion,
      idempotent `Close`, and destructor close without an observable error.
      R8: EOF locks the family; explicit `Close` flushes.
- [x] Negative fixtures for unknown modes, invalid byte counts, invalid UTF-8,
      unsupported paths, and excluded directory / seek operations.
      R8: `Open(".")` is `Error` on Unix; fixture prints Open and
      `DeleteFile` separately.

## Sprint 6 — `BNData`

- [x] Module layout and resolution: user sources beneath `modules/`; language
      standard-library sources beneath `modules/bn/`. `IMPORT BNData AS Data`
      resolves logical `BNData` to `modules/bn/BNData.bn`.
- [x] Module surface: `IMPORT BNData AS Data`, owned `DataFrame`, and
      `DELETE` lifecycle.
- [x] CSV: `ReadCSV` / `WriteCSV`, UTF-8, quoting, separator validation,
      headers, and ragged-row errors; read columns begin as `STRING`.
      **Reopened: R4** (`WriteCSV` success-on-I/O-failure; unterminated
      quotes raise).
- [x] Frame storage: `Add*Column`, counts, names, and typed cell getters.
- [x] Conversion and statistics: `ConvertToInteger`, `ConvertToFloat`, and
      all documented column reductions with atomic conversion failure.
      **Reopened: R6** (empty / all-`NA` reductions vs `BNMath`).
- [x] Interop: `CopyIntegerColumn` / `CopyFloatColumn`, `Select`, and
      `Slice`, including ownership and bounds fixtures.
      **Reopened: R6** (negative `Select`; empty-frame `Slice`).
- [x] Composition: `AppendRows`, `AppendColumns`, `Join`, `LeftJoin`,
      `RightJoin`, `FullJoin`, and `Transpose`, with label and missing-value
      fixtures.

## Sprint 7 — Jupyter kernel

A notebook is another host of `bn`, not a second language. The Rust crate
does not link ZeroMQ.

- [x] Python package `bn-kernel`: cell → temp `.bn` → `bn run`. Kernelspec
      `bn`. The Rust `bn` crate stays zero-dependency; the Python package
      uses `pyzmq` for the wire path.
- [x] A cell is a **complete program** with `FUNCTION Start()`. No top-level
      statements, no state between cells.
- [x] Stream I/O to the cell without pipe deadlock. `INPUT()` uses Jupyter
      `input_request` / `input_reply` (not `execute_request.content.stdin`).
      Heartbeat, interrupt, and shutdown stay live during execute. **R1, R2.**
      `PrintAt` / `NumCols` / `NumRows` fail at the call (not a TTY).
      `HOST.FileSystem` is unavailable (denied on import, R8).
- [x] Installable kernelspec (`pip` / kernelspec data files),
      `kernel_info` handshake, execute/diagnostic integration test, and
      pre-`Start` rejection of `HOST.FileSystem` imports. **R1, R2;**
      import denial is R8.

Not in this kernel: accumulated declarations across cells, implicit `Start`,
HTML grid for `PrintAt`, in-process Rust ZMQ kernel.

## Sprints 8–9 — Compiler track

After console, Random, FileSystem, BNData, and the kernel:

- [x] Before lowering: record the approved Rust-to-LLVM integration strategy,
      supported LLVM version, target triples, host/CI matrix, and capability
      table. Do not add a dependency until this decision exists.
- [x] Define `bn build` artifacts: Windows (PE/COFF), macOS (Mach-O), Linux
      (ELF), WebAssembly (`wasm32`). Record which `HOST` capabilities each
      target provides.
- [x] `bn build` validates the frontend and refuses with an explicit diagnostic
      until LLVM lowering is implemented.
- [x] Lower typed BN IR to LLVM IR. **R10** (correctness of the subset
      already emitted) before **R11** (typed instruction lowering).
- [x] Emit native code and WASM. Interpreter/compiler parity on the
      conformance suite. **R12.**
- [x] Compile-time diagnostics for unsupported host capabilities.

JVM remains a possible later backend from BN IR, **not** an LLVM OS, and
**not** 0.2.

**Done when:** `bn build` from the same BN IR matches `bn run` on the accepted
LLVM targets for programs that do not require a TTY, and TTY/FS programs have
a documented compile-time or runtime capability failure.

## Release closure backlog

R1–R18 are historically closed, but U1 is now the urgent acceptance gate.
After U1 closes, R19 remains: capture the Windows real-TTY `NumCols` /
`NumRows` / `PrintAt` traces. The historical `v0.1.0` publication
authorization is independent and does not block 0.2.

No checked item may be reopened without a concrete specification conflict,
implementation defect, or missing verification artifact.

## Sprint 10 — VS Code Extension

- [x] Create VS Code language extension project in `plugins/vscode/`.
- [x] Implement syntax highlighting via TextMate grammar using
      `docs/library/basicnext.tmLanguage.json`. **Evidence reopened by
      U1.5–U1.6.**
- [x] Implement on-save linting diagnostics by parsing `bn check` output.
      Remaining host issues (dirty buffer, missing `bn`, DAP lifetime)
      are **R17**, not a reopen of this checkbox.

## Not scheduled

- `HOST.Network`, GPU, DOM.
- C FFI (`todo/proposals/c-ffi.md`).
- TZDB-backed zone conversion (0.1 `TIMEZONE` stays an identifier).
- `MATCH`, `ENUM`, generic classes, variable-size collections.

## 0.3 scope (future)

- `BNText` Markdown values (`todo/proposals/bntext-markdown.md`).
- `PARALLEL` (`todo/proposals/parallel-computing.md`).
- Package manifest/registry, formatter/LSP.
- Portuguese aliases for `BNMath`.
- ncurses as a `HOST.Console` implementation.
- Jupyter REPL (state between cells, loose statements) and a native ZMQ
  kernel inside `bn`.
- Directory APIs, `Seek`, `PROTECTED`, downcast.

## Archived resume point

R19 moved to active 0.3 gate `G0.1`: capture Windows real-TTY `NumCols` /
`NumRows` / `PrintAt` traces with `tests/windows-console-evidence.ps1` in
Windows Terminal or `conhost`. U1.1–U1.11 were closed; U1.12 remained this
item. R1–R18 were historically done.
