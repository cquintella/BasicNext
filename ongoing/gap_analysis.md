# Basic Next 0.2 — Gap analysis

Initial audit: 2026-08-28. Reconciled: 2026-08-29.

This is a code-and-contract audit of the 0.2 tree against
`docs/language/0.2/0.2.ebnf`, `docs/language/0.2/0.2.md`,
`docs/language/0.2/keywords.md`, and the library contracts in `docs/library/`.
It is not a sprint audit and does not replace those contracts.

The archived 0.2 delivery bucket lives in
[`bucket-0.2.md`](../archive/project/bucket-0.2.md). Checked sprint
items that this file reopens have a concrete specification conflict,
implementation defect, or missing verification artifact.

## How to read this file

Severity:

- **P0** — product path does not work, or valid programs execute with the
  wrong accepted semantics.
- **P1** — contract violation, wrong result, or `bn check` accepts/rejects
  the wrong program.
- **P2** — unfinished surface, architectural gap, or fragile host behaviour.
- **P3** — documentation drift, leftover code, test holes.

Locations are `path:line` in the tree as of this date.
Each numbered finding retains its original audit evidence. Its leading
**Status** paragraph is authoritative after remediation.

There is no data race in the Rust interpreter: execution is single-threaded
(`Cell` for `HOST.Random`, no shared locks). Concurrency defects are in the
Jupyter kernel (blocked event loop and pipes) and, more weakly, in the
VS Code linter.

## Current resume point

R1–R18 are resolved. Resume at R19: Windows real-TTY evidence for console
dimensions and positioned output. The original attack order is preserved by
the numbered findings and their R mappings.

## What is solid

- Pipeline lexer → parser/AST → semantics → typed BN IR → interpreter, with
  source spans.
- 0.2 OO surface that the fixtures cover: `EXTENDS`/`SUPER` as syntax,
  virtual dispatch after construction, read-only `s[i]`, `HOST.Args`.
- Heap generations plus `destroying`: a destructor may still read the
  payload; reentrant `DELETE` is `DOUBLE_DELETE`.
- `HOST.FileSystem` text/binary family, idempotent `Close`.
- Interpreter integer overflow uses `checked_*`, not wrap.
- Kernel HMAC uses `compare_digest`; stdin is ROUTER; `--no-filesystem`
  denies an executable `HOST.FileSystem` import before `Start`.
- VS Code save-race uses `document.version` and has a Node test.

`BNData.bn` placeholder bodies (`RETURN 0`, `NEW DataFrame()`) are API
only. The runtime intercepts `#mod.DataFrame.*` and `ReadCSV`/`WriteCSV`
through `bndata_providers`.

---

## P0

### 1. Jupyter kernel has no `kernel_info_request`

**Status: Resolved by R1.** The live wire suite covers `kernel_info_request`
and `kernel_info_reply`.

`plugins/jupyter/bn_kernel/jupyter.py:115-119` handles `shutdown_request` and
`execute_request` only. Grep finds no `kernel_info` in the tree.

JupyterLab / `jupyter_client.KernelClient.wait_for_ready()` send
`kernel_info_request` and wait for `kernel_info_reply` with
`language_info`. Without that reply the `bn` kernelspec does not complete
handshake.

`tests/test_jupyter.py` never sends `kernel_info_request`. Sprint 7 audit
marks complete; `bucket.md` Sprint 7 items remain unchecked. The bucket
checkboxes are the more accurate record.

### 2. Kernel stdout/stderr deadlock

**Status: Resolved by R2.** Child streams are drained concurrently and the
large-output wire test completes without starving heartbeat.

`plugins/jupyter/bn_kernel/jupyter.py:72-95` uses `Popen` with all three stdio pipes and
drains stderr to EOF before `stdout.read()`.

A cell that `PRINT`s more than the OS pipe buffer (~64 KiB) and does not
write stderr blocks in the child on stdout and in the parent on
`stderr.readline()`. There is no timeout. Heartbeat, shutdown, and
interrupt are not polled during `_execute`.

The JSON-lines path in `plugins/jupyter/bn_kernel/kernel.py` uses `subprocess.run` /
`communicate` and does not have this bug. The ZMQ path does.

### 3. `NEW Derived` runs field initializers before the base constructor

**Status: Resolved by R3.** Construction and destruction order, including
pinned dispatch, are covered by runtime tests.

`docs/language/0.2/0.2.md` Construction:

1. Allocate the most-derived object.
2. Evaluate `SUPER(...)` (explicit or implicit): base field initializers,
   then the base constructor body, and so on.
3. Run derived field initializers.
4. Run the rest of the derived constructor after `SUPER`.

`src/ir.rs:2124-2148` calls `Derived.$fields` then `Derived.CONSTRUCTOR`.
`src/ir.rs:615-633` makes `Derived.$fields` run `Base.$fields` and then
the derived initializers.

Observed order: allocate → base fields → **derived fields** → base
constructor → derived constructor body.

A subclass field `PUBLIC label AS STRING = SELF.name` therefore sees the
default `""` if `name` is assigned in the base constructor. During the
base constructor, subclass fields have already run, against the paragraph
that says they have not.

`lifecycle_dispatch` (`src/runtime.rs:4078-4086`) pins `.CONSTRUCTOR` and
`.DESTRUCTOR` only. A method call from a field initializer uses the most
derived class.

Sprint 1 inheritance is checked in `bucket.md`. This item reopens the
construction contract.

---

## P1 — interpreter and language

### 4. `WriteCSV` reports success after I/O failure

**Status: Resolved by R4.** CSV writing commits only after successful I/O and
returns `Error` on failure.

`src/runtime.rs:1533-1540` loops `file_call("FS.File.WriteLine", ...)`.
`WriteLine` returns `Ok(Value::Error { ... })` on a closed file or disk
error. `?` does not see that. The function always returns `VOID`.

Contract: `WriteCSV … AS VOID OR Error`. Partial CSV plus silent success.

### 5. Unterminated CSV quotes raise `CSV_ERROR`; ragged rows return `Error`

**Status: Resolved by R4.** Both malformed CSV forms return language-level
`Error` values and have negative runtime coverage.

`src/runtime.rs:1422-1423`, `1579-1580`. `docs/library/error.md`: file and
CSV operations return `T OR Error`; they do not raise exceptions.

### 6. `WriteBytes` / `ReadBytes` abort on I/O

**Status: Resolved by R4.** Byte I/O failures return `Error` without committing
partial state.

`src/runtime.rs:2317-2320` maps `write_all` to `runtime_error("IO_ERROR")`.
`host.md`: `WriteBytes … AS VOID OR Error`. Text `Write`/`WriteLine` already
return `Value::Error`.

`ReadBytes` (`2248-2250`) does the same. Its declared type is
`INTEGER OR EOF`, so a disk error has no legal `Error` channel — closed-file
and text-mode paths still return `Value::Error` anyway.

### 7. `IS FS.File` and `IS DataFrame` are never true

**Status: Resolved by R5.** Runtime identity tests recognize both resource
types.

`src/runtime.rs:3656-3673` `is_value` has no `Value::File` /
`Value::DataFrame` arms. The IR for `IS FS.File` joins tokens with spaces
(`"FS . File"`, `src/ir.rs:1544-1552`). `IS Error` works; the other
alternative of `FS.File OR Error` does not.

### 8. Identity equality of `File`, `DataFrame`, and `Error` is wrong

**Status: Resolved by R5.** Resource identity and `Error` value equality have
explicit runtime rules and tests.

`src/runtime.rs:3611-3653` compares `Object`/`Pointer` by handle. There is
no `File`/`DataFrame`/`Error` arm → `_ => false`. Two references to the
same `FS.File` compare unequal.

`Value::Handle` compares equal regardless of `type_name`. Default `VOID`
and default `Error` (both `Handle` in `empty_named`) compare equal.

### 9. `Select` with a negative index aborts; positive OOB returns `Error`

**Status: Resolved by R6.** Negative and positive out-of-range indices return
`Error` consistently.

`src/runtime.rs:3730-3736` vs `2129-2138`. `bndata.md`: out-of-range
indices return `Error`. `Slice` already returns `Value::Error` for a
negative index.

### 10. DataFrame reductions do not follow `BNMath` on empty / all-`NA`

**Status: Resolved by R6.** Empty and all-`NA` reductions follow the BNMath
contract and are covered by fixtures.

`src/runtime.rs:1981-1986`, `2023-2027`. `Mean` on empty →
`Error "empty numeric column"`. `math.md` / `bndata.md`: empty `MEAN` /
`MEDIAN` / `QUARTILE*` / `RANGE` yield `NAN`. An integer column of only
`NA` becomes `"column is not numeric"`.

### 11. `FOR EACH` is not checked

**Status: Resolved by R9.** Iterable shape, binding type, and binding
immutability are checked statically.

`src/semantic.rs:2198-2209` evaluates the iterable and declares a
**mutable** local. Spec: read-only binding; declared type is the vector
element; only a fixed-length vector. `item = …` and
`FOR EACH x AS INTEGER IN "abc"` pass `bn check`.

### 12. Postfix `AS` has no static conversion rule

**Status: Resolved by R9.** Unsupported conversions fail during `bn check`.

`src/semantic.rs:2688-2690` types the cast as the target and ignores the
source. `"hi" AS INTEGER` passes `bn check` and fails at runtime. 0.1
explicit conversion is a static error.

### 13. `CLS` / `BEEP` remain reserved words and 0.1 statements

**Status: Resolved by R9.** The withdrawn statements are rejected and the
method forms remain available.

`src/token.rs:57-62`, `src/parser.rs:1285-1304`. 0.2 EBNF `reserved-word`
does not include them. `CLS(HOST.Console)` never becomes a call.
`FUNCTION CLS()` is a lexical error. `PRINTAT` is already unreserved.

### 14. Any `=` on the line is parsed as assignment

**Status: Resolved by R9.** Statement-form parsing distinguishes assignment
from equality inside call arguments.

`src/parser.rs:1333-1378` scans the whole line for `=` / `+=` / …,
including inside `()` and `[]`. `DoWork(flag = TRUE)` is a valid call and
fails to parse.

### 15. Extra tokens after `CLASS` / `STRUCT` / `INTERFACE` are dropped

**Status: Resolved by R9.** Unexpected header tokens are syntax errors.

`src/parser.rs:647-665`, `1522-1526`. `CLASS Dog Extra` parses as
`CLASS Dog`.

### 16. Floating literal `1.` is accepted

**Status: Resolved by R9.** A fractional digit is required after the decimal
point.

`src/lexer.rs:300-313`. 0.2 EBNF requires a digit after the point.
`.5` is rejected (correct). `1.` is not.

### 17. User `INTEGER[]` / `T[]` is a variable-length vector

**Status: Resolved by R9.** Variable-length vector declarations are restricted
to the BNData provider surface.

`src/semantic.rs:3786-3803` uses `u64::MAX` for empty brackets. Variable
`TYPE[]` is outside 0.2. `POINTER TO INTEGER[]` is a different production
and is valid. `BNData.bn` uses `INTEGER[]` as library API; the frontend
does not distinguish library from user code.

### 18. Imported-interface upcast matches the last name segment

**Status: Resolved by R9.** Imported interface identity includes its module.

`src/semantic.rs:3391-3401`. `CLASS Dog IMPLEMENTS Pets.Named` is
assignable to `Other.Named` if the local name is `Named`.

### 19. `EXTENDS Data.DataFrame` is not rejected

**Status: Resolved by R9.** Standard and host classes cannot be extended.

`src/semantic.rs:1524-1558`. 0.2 forbids extending a host or standard
library class (`FS.File`, `Data.DataFrame`, `Error`). `Error` / `FS.File`
fail for other reasons. `DataFrame` is a real `CLASS` in `BNData.bn`.

### 20. `SUPER` in a destructor is not a static error

**Status: Resolved by R3/R9.** Destructor chaining is implicit and explicit
`SUPER` is rejected.

`src/semantic.rs:1661-1710`, `2606-2608`. Spec: no `SUPER` in a
destructor; the chain is implicit. IR can lower it as a base constructor
call.

### 21. Unix console size reads stdin; the TTY guard reads stdout

**Status: Partially resolved by R7; Windows evidence remains R19.** Unix uses
an stdout `ioctl` and the stdin-pipe/stdout-TTY fixture passes. The Win32 query
is implemented, but real-TTY capture is still required.

`src/runtime.rs:1070-1076`, `1108-1113`, `2863-2874`. `console.md` asks
for ioctl / Win32 on the window. Unix spawns `stty size` with
`stdin(Stdio::inherit())`. `echo | bn run` on a terminal: stdout is a TTY,
`stty` sees a pipe → `HOST_CAPABILITY_UNAVAILABLE`. Windows uses
`STD_OUTPUT_HANDLE`. Real-TTY Windows evidence is still missing
(`bucket.md` P1).

### 22. Filesystem capability is “IR used the constant”, not “imported”

**Status: Resolved by R8.** Capability denial is derived from the executable
module import before `Start`.

`src/runtime.rs:349-368` scans for `Constant::Type("HOST.FileSystem")`.
`host.md`: fail before `Start` when the module **imports** the capability.
An import with no use may not emit the constant and can pass
`--no-filesystem`.

### 23. Open of a directory on Unix can succeed

**Status: Resolved by R8.** Directory paths return `Error` on open.

`src/runtime.rs:1159-1160`. `Exists` treats a directory as `FALSE`.
`open(".")` on Unix often succeeds. The directory-open fixture can pass
on a single `PRINT` even if only `DeleteFile` failed.

### 24. Text/binary family is asymmetric on EOF

**Status: Resolved by R8.** The first successful family operation, including
EOF, locks the file family consistently.

`src/runtime.rs:1319-1334`, `2248-2256`. `ReadBytes` with zero bytes marks
binary then returns `EOF`. `ReadLine` on empty returns `EOF` without
marking text, so a later `ReadBytes` still proceeds.

### 25. `Close` never returns `Error` and does not flush

**Status: Resolved by R8.** Explicit close flushes and reports failures as
`Error`.

`src/runtime.rs:1252-1255` sets `file = None`. Drop of `std::fs::File`
ignores `close(2)` errors. Contract: `Close() AS VOID OR Error`, flush
and release. Destructor close without an observable error is allowed;
explicit `Close` is not the same.

### 26. `HOST.Clock.Timestamp` before 1970 becomes `0`

Resolved by R14. System time before `UNIX_EPOCH` is converted to negative
signed milliseconds, with a focused pre-epoch test.

### 27. `Timestamp.Parse` rejects 1–2 fractional digits

Resolved by R14. RFC 3339 fractions accept one or more digits; discarded
sub-millisecond digits must remain zero as required by `temporal.md`.

### 28. `Slice` on a frame with no columns skips the row check

**Status: Resolved by R6.** Empty-frame row ranges are validated.

`src/runtime.rs:2129-2133`. `NEW DataFrame()` then `Slice(0, 1, 0, 0)`
returns an empty frame instead of `Error`.

### 29. `Error` values do not coerce to `Type::Named("Error")` outside an alternative

**Status: Resolved by R5.** Error values coerce and participate in member and
identity operations consistently.

`src/runtime.rs:3379-3426`. Default `Error` is `Value::Handle`, so
`e IS Error` is false and `e.Code` is missing. Tests that `PRINT`
`.Message` on an alternative bypass `coerce`.

---

## P1 — compiler (`bn build`)

### 30. Pattern matcher, not typed lowering

Resolved by R11. `src/llvm.rs` now lowers the supported typed instruction
classes directly. Unsupported instruction classes retain
`BUILD_LOWERING_UNAVAILABLE` with the failing instruction name.

Outside the subset: objects, vectors, `NEW`/`DELETE`, virtual dispatch,
`HOST.Console`, `HOST.FileSystem`, `BNData`, dynamic loops, unfolded
calls.

### 31. `DIV` / `%` are truncated, not Euclidean; division by zero is UB

**Status: Resolved by R10.** Accepted native lowering uses checked Euclidean
division/remainder and diagnoses zero divisors.

Interpreter: `checked_div_euclid` / `checked_rem_euclid`,
`DIVISION_BY_ZERO`, `NUMERIC_OVERFLOW` (`src/runtime.rs:3271-3279`).

`src/llvm.rs:398-404`:

- `DIV` / `Slash` on `i64` → `sdiv`
- `Percent` → `srem`
- integer `add`/`sub`/`mul` wrap
- `SHR` → `ashr` (interpreter is a logical shift of the width)

`(-5) DIV 3` is `-2` under `bn run` and `-1` under `bn build`. `x DIV 0`
is a diagnostic in the interpreter and undefined in LLVM.

The constant folder (`src/llvm.rs:1220-1224`) treats integer `Slash` as
truncated `checked_div`. Language `/` is always floating.

This is not “subset incomplete”: programs the compiler **accepts** already
diverge.

### 32. Multiple `PRINT` in one block; duplicate SSA names

**Status: Resolved by R10.** Every print is emitted and SSA identifiers are
unique.

`constant_print_values` keeps the first `Print` and drops the rest.
`PRINT 1` then `PRINT 2` in the same block becomes the first only.

`scalar_module` names `printf` results from the `ValueId`
(`src/llvm.rs:518-522`). Two prints of the same value emit `%printN`
twice: invalid IR, `clang` rejects.

### 33. Float `PRINT` uses libc `%g`

**Status: Resolved by R10.** Compiled float rendering matches the interpreter
on the accepted subset.

Interpreter `render` (`src/runtime.rs:4023-4035`): `1.0` → `"1.0"`,
`NAN`/`INF` as tokens. LLVM `%g` prints `1` and host NaN/Inf. Parity
fixtures use `3.75` and a dedicated `%.17g` random path, so they pass.

### 34. Unseeded compiled `HOST.Random` is always `1`

**Status: Resolved by R10.** The accepted compiler path preserves the host
seed rule; deterministic parity uses explicit seeding.

Interpreter: time XOR pid, never zero (`src/runtime.rs:130-136`).
`src/llvm.rs:220-222` stores `i64 1`. Spec allows the host to choose;
parity with `bn run` exists only after `Seed`.

### 35. wasm32 emits an object, not an executable

Resolved by R12. The wasm32 object is linked by `wasm-ld`; executable parity
runs in the documented `bin/bn-wasm` Node.js host.

---

## P1 — kernel, CLI, CI

### 36. Heartbeat and stdin share the execute thread

**Status: Resolved by R2.** Heartbeat has a dedicated thread; execution,
interrupt, shutdown, and stdin remain responsive.

`plugins/jupyter/bn_kernel/jupyter.py:49-66`, `97-129`. Heartbeat is a `REP` on the main
poller. `_execute` and `_input_reply` are synchronous. `_input_reply` polls
only stdin, forever. A dead child after `BN_INPUT_REQUEST`, or a frontend
that never replies, hangs the kernel. Jupyter then marks it dead.

### 37. Kernelspec is not installed by the package

**Status: Resolved by R1/R2.** Packaging, kernelspec, handshake, execution,
interrupt, and shutdown are covered by the live suite.

`plugins/jupyter/pyproject.toml` ships `bn_kernel*` only.
`plugins/jupyter/kernelspec/kernel.json` assumes
`python3 -m bn_kernel` and `bn` on `PATH`.
`tests/test_jupyter.py::test_execute_and_shutdown` never sends
`shutdown_request`; it `terminate()`s the process.

### 38. CI builds binaries and publishes; it does not test

Resolved by R15. `.github/workflows/binaries.yml` has a blocking quality job
for Rust, kernel, VS Code, native compiler parity, and linked wasm parity. It
installs LLVM/Clang 22, all build jobs depend on quality, and publication is
disabled for pull requests and depends on both quality and builds.

---

## P2

### 39. `BNMath` is still a compiler builtin

Resolved by R13. `modules/bn/BNMath.bn` defines the exported semantic API;
removing an export makes use of that name fail during semantic analysis.
Semantic overload rules, IR constants, and runtime intrinsics activate only
for the resolved BNMath `ModuleId`; an unrelated module with the same member
name follows normal module execution.

### 40. `--help` still says `bn build` refuses until LLVM exists

Resolved by R16. User help and `bn(1)` describe the accepted LLVM subset and
`--no-filesystem`; the kernel-private `--jupyter-stdin` boundary is documented
in `docs/project/kernel.md`.

### 41. `docs/project/usage.md` is still 0.1

Resolved by R16. The current usage guide documents the 0.2 CLI, `HOST.Args`,
native and wasm32 builds, modules, capability boundaries, and explicit backend
limits.

### 42. `docs/project/kernel.md` is stale

Resolved by R16. The current kernel guide documents Jupyter
`input_request`/`input_reply`, call-time positioned-console failure, and
pre-`Start` filesystem denial. The legacy JSON-lines launcher remains a
separate adapter and validates missing option values.

### 43. VS Code: save-race is handled; other races are not

Resolved by R17. Run/build save a dirty document and abort on save failure;
missing `bn` publishes a workspace diagnostic. The launch-only DAP no longer
equates terminal creation with process termination, and its limits are stated
in the extension README and covered by Node tests.

### 44. Module graph `normalize` is identity

Resolved by R14. Existing files are canonicalized before module identity is
assigned. Standard modules resolve from `modules/bn` above the source or the
`bn` executable, never through a process-working-directory overlay.

### 45. Trailing comma in `FUNCTION` parameters is accepted

Resolved by R9. A comma must be followed by another parameter, and
`tests/grammar/invalid/trailing-function-comma.bn` covers the rejection.

### 46. `POINTER TO` named-type / `VOID` is a static error

Resolved by R18 and the BDFL decision of 2026-08-29. Declared named elements
are accepted. `POINTER TO VOID` is the C-style opaque form: compatible-shape
typed pointers convert in both directions, direct opaque indexing is a static
error, and `NEW VOID` is invalid. Grammar, semantics, runtime type tests, and
positive/negative fixtures now agree.

### 47. Compiler parity is a small native subset

Resolved for the accepted compiler subset by R12. Native parity grows with
typed lowering, and `tests/test_wasm_parity.py` executes linked artifacts for
empty, scalar print, Euclidean arithmetic, seeded random, `HOST.Args`, and
`INPUT`. Objects, vectors, BNMath, BNData, filesystem, and dynamic loops remain
explicitly outside the accepted compiler subset.

### 48. `HOST.Args[i] = …` uses the wrong diagnostic

Resolved by R9. Assignment-target validation reports the immutable/not-lvalue
diagnostic, and the negative fixture asserts the exact error code.

---

## P3 — drift and leftovers

| Item | Resolution |
| --- | --- |
| Sprint 7 bucket/audit status | Reconciled after R1, R2, and R8; both are complete. |
| Resume point | Reconciled to R19, the sole remaining 0.2 evidence item. |
| `examples/lexical.bn` imported `HOST.Main` | Removed; active examples and usage use `HOST.Args`. |
| Dead `HOST.Main.Argument*` runtime/IR | Removed. |
| Man page tutorial and syntax | Points at `docs/book/en/toc.md`; `mandoc -Tlint` passes. |
| Missing WBS paths | Active records and sprint audits point at `ongoing/WBS-0.2.md`. |
| `AGENTS.md` worktree state | Excluded from release closure by the BDFL; R16 does not change it. |
| `analise.md` 0.1 readiness note | Gitignored and outside the tracked 0.2 records. |
| Capability diagnostics said “0.1” | Updated to 0.2. |
| Kernel dependency description | Rust remains dependency-free; the Python adapter documents `pyzmq`. |
| Launcher missing option values | `-f` and `--bn` return exit 2 without traceback; covered by tests. |
| `RowCount` / `NumCols` `Int32` range | Checked through the shared `INTEGER` count conversion; overflow is `NUMERIC_OVERFLOW`. |
| Heap payload reservation | Uses `try_reserve_exact`; failure is `ALLOCATION_TOO_LARGE` without heap mutation. |

Historical sprint audits under `done/project/audit-sprint-*.md` are
evidence records. They are not updated by this file. When a checked
bucket item conflicts with a finding here, this file wins until the
bucket is reconciled.

---

## Bucket vs this file

Archived work program: [`bucket-0.2.md`](../archive/project/bucket-0.2.md) items
**R1–R19**. Each gap number
above maps into that program (R1 = #1, R2 = #2/#36/#37, R3 = #3/#20, …).

Do not mark an archived bucket item complete while a requirement in this file
for that surface is still open.
