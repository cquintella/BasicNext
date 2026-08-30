# WBS — Basic Next 0.2

This WBS applies a lightweight PMI approach: it organizes scope by deliverable,
not by people or dates. Each level-2 item is a GitHub Project card. Execution
is sprint by sprint; stop when a spec gap blocks the next sprint.

## Scope baseline

**Status:** Accepted for 0.2 planning with the language contract in
[`docs/language/0.2/0.2.md`](../docs/language/0.2/0.2.md).

**Authority:** [`0.2.ebnf`](../docs/language/0.2/0.2.ebnf), [`0.2.md`](../docs/language/0.2/0.2.md),
[`keywords.md`](../docs/language/0.2/keywords.md). Library and host contracts in
`docs/library/`. 0.1 remains frozen in `docs/language/0.1/`.

### In scope for 0.2

- Single class inheritance (`EXTENDS`, `SUPER`, virtual override, upcast).
- Read-only `STRING` indexing by Unicode scalar.
- Console methods on `HOST.Console`: `Cls`, `Beep`, `PrintAt`, `NumCols`,
  `NumRows`. Withdrawal of `CLS` / `BEEP` statements.
- `BNMath.VAL`, range constants, descriptive statistics; withdrawal of
  `Float.TryParse`.
- `HOST.Random` (`Random`, `Seed`).
- `HOST.FileSystem` and class `FS.File` (no I/O keywords).
- `BNData` (`DataFrame`, CSV as string columns).
- Jupyter kernel `bn-kernel` (cell = complete program).
- `bn build` contract and LLVM lowering from typed BN IR.

### Explicitly outside 0.2

- Packages, registry, LSP, formatter.
- `HOST.Network`, C FFI, TZDB conversion, GPU, DOM.
- `PARALLEL`, JVM backend, `MATCH` / `ENUM` / generics.
- Variable-size collections, `PROTECTED`, downcast, `CHAR`.
- Directory APIs, `chmod`, `ChangeDirectory`, `Seek`.
- Persistent notebook REPL, ncurses, Portuguese `BNMath` aliases.

## Delivery objective

Specify and implement Basic Next 0.2 on the existing pipeline
(lexer → parser/AST → semantics → typed BN IR → interpreter), then a
Jupyter host using `pyzmq`, then `bn build` from the same IR.

## Release acceptance criteria

- **Specification:** `0.2.ebnf`, `0.2.md`, and `keywords.md` agree; every
  accepted 0.2 rule has defined semantics; withdrawn 0.1 forms are named.
- **Interpreter:** `bn check` / `bn run` implement the 0.2 language and host
  surfaces with source-spanned diagnostics.
- **Conformance:** positive and negative fixtures for each sprint exist and
  pass.
- **Kernel:** `bn-kernel` executes a complete-program cell and reports the
  same diagnostics as `bn run`.
- **Compiler:** `bn build` from BN IR matches `bn run` on the accepted LLVM
  targets for programs that do not require a TTY or filesystem the target
  lacks.
- **Release:** `v0.2.0` documents installation, usage, and capability limits.

## Work breakdown structure

### 1. Delivery management

- **1.1 Scope and acceptance — Complete:** this WBS and `0.2.md`.
- **1.2 Delivery control — Active:** GitHub Project cards, one sprint at a
  time. 0.1 leftover: maintainer authorizes `v0.1.0` independently.

### 2. Language specification 0.2

- **2.1 Normative EBNF** — `docs/language/0.2/0.2.ebnf`.
- **2.2 Language semantics** — `0.2.md` (inheritance, `SUPER`, string index,
  console methods, withdrawals).
- **2.3 Keywords** — `keywords.md`.
- **2.4 Library and host contracts** — `math.md`, `host.md`, `console.md`,
  `bndata.md`, `error.md`.

### 3. Language on the interpreter (sprint 1)

- **3.1** Parse `EXTENDS` / `SUPER`; reject `SUPER` as a value and as a
  non-first constructor statement.
- **3.2** Semantics: single base, acyclic graph, override, visibility,
  implicit `SUPER()`, destructor chain, upcast, virtual dispatch,
  `IMPLEMENTS` with `named-type` (`alias.Interface`).
- **3.3** `STRING` read-only index; assignment to `s[i]` is a static error.

### 4. BNMath 0.2 (sprint 2)

- **4.1** `VAL`, range constants, descriptive statistics.
- **4.2** Remove `Float.TryParse` from the 0.2 surface; update
  `examples/rpn-calculator.bn`.

### 5. Host capabilities (sprints 3–5)

- **5.1** `HOST.Random` with injectable provider.
- **5.2** Console methods; TTY vs pipe fixtures; withdraw `CLS`/`BEEP`
  statements.
- **5.3** `HOST.FileSystem` and `FS.File`.

### 6. BNData (sprint 6)

- **6.1** Standard-library module resolution: user modules beneath
  `modules/`; language modules beneath `modules/bn/`, including
  `modules/bn/BNData.bn` for logical import `BNData`.
- **6.2** `DataFrame` class, CSV read/write (string columns), stats methods,
  copy-out, `Select` / `Slice`.

### 7. Jupyter kernel (sprint 7)

- **7.1** Python `bn-kernel` wrapping `bn run`; cell = complete program;
  no TTY positioned I/O; no `HOST.FileSystem`.

### 8. Compiler track (sprints 8–9)

- **8.1** `bn build` artifacts (PE/COFF, Mach-O, ELF, `wasm32`) and
  per-target capability table.
- **8.2** LLVM lowering of typed BN IR; native and wasm emission; parity.

### 9. Conformance and release

- **9.1** Fixtures for every 0.2 surface.
- **9.2** `v0.2.0` notes and publication.

## Main dependencies

```text
2 spec freeze
  → 3 inheritance + string index
  → 4 BNMath
  → 5.1 Random → 5.2 Console → 5.3 FileSystem
  → 6 BNData
  → 7 kernel
  → 8.1 build contract → 8.2 LLVM
  → 9 release
```

## Sprint program

| Sprint | Status | Deliverable | Done when |
| --- | --- | --- | --- |
| 0 Spec freeze | Complete | Documents in §2 | EBNF, `0.2.md`, keywords, library docs agree |
| 1 Language OO | Complete | `EXTENDS`, `SUPER`, `s[i]` | Fixtures +/- check and run |
| 2 BNMath 0.2 | Complete | `VAL`, constants, stats; no `TryParse` | Bucket numeric fixtures pass |
| 3 `HOST.Random` | Complete | `Random` / `Seed` | Seeded sequence is deterministic |
| 4 Console methods | Evidence pending (R19) | `Cls` / `Beep` / `PrintAt` / size | Windows real-TTY capture; implementation and Unix evidence pass |
| 5 `HOST.FileSystem` | Complete | `Open` / `File` / `Exists` / `DeleteFile` | `OR Error`; idempotent `Close` |
| 6 `BNData` | Complete | CSV + `DataFrame` | String columns; no `TYPE[]` |
| 7 Jupyter | Complete | `bn-kernel` | 11 kernel/wire tests pass; Rust `bn` stays dependency-free |
| 8 `bn build` contract | Complete | Artifacts + capability table | Documented; unsupported typed IR is refused explicitly |
| 9 LLVM | Complete | IR → native/wasm | Seven native/WASM parity tests match `bn run` |
