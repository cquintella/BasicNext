# Bucket book — Reorganize the tutorial book for bottom-up teaching

**Status:** executed 2026-10-27 (Carlos: "execute o bucket, mas nao toque no codigo do basicnext nessa execucao"); awaiting review and commit in the book repository. Documentation only, no language change; no file of the BasicNext code was changed.
**Repository:** `bn/docs/book/` (`cquintella/basicnext-book`), not this workspace. Paths below are relative to `bn/docs/book/`.
**Skill:** `technical-book-editor` for every activity.

**Claim:** the book teaches Basic Next in an order where no chapter uses a construct a later chapter introduces, each topic has one home, and the appendices are reference only. Content is moved and tightened, not rewritten from scratch.

**Not here:** new language material beyond what the book covers today; translation; the HTML/PDF build pipeline beyond regenerating `BasicNext_Book.html`; changes to `language/0.6/`.

---

## Findings that motivate the bucket

Forward references (a construct used before the chapter that teaches it):

| Chapter today | Uses | Taught in |
| --- | --- | --- |
| 2 Common Concepts | `OR` alternatives, `IS`, `NULL`/`NA`/`EOF`, vector and `STRUCT` literals, `Date.Parse`, `IMPORT BNMath`, `IMPORT HOST.Console`, cross-file `EXPORT CONST` | 4, 5, 8 |
| 3 Control Flow | functions with parameters and `RETURN`, vectors, `FOR EACH`, `LEN`; `++`/`--` hidden under `WHILE` | 4, 5, 8 |
| 4 Compound Data | `IMPORT HOST.FileSystem`, `FS.Open`, method call `file.Close()` | 5, 6, 8/9 |
| 5 Functions/Modules | `CLASS`, `PUBLIC`, `CONSTRUCTOR`, `NEW`, `PRIVATE_ACCESS` | 6 |
| 6 OO | destructor "by `RELEASE`" | 7 |
| 7 Memory | constant parameters (a function topic); internal representation of alternatives (an implementation topic) | — |

Topics split across places: modules (ch 1, 2, 5, Appendix I — the search path is given with 5 layers in ch 5 and 3 in Appendix I); `IS` narrowing (ch 2, 3, 4); return analysis (ch 3, 5); ARC (ch 1, 7); `HOST.FileSystem` (ch 8, 9); `BNDispatch` (ch 9, Appendix J); temporal types (ch 2, 8, Appendix A).

Abstract before concrete: ch 1 opens with taxonomy, application domains, ARC and the full `BN*` list before the first program. Ch 8 is a catalogue. Appendix letters do not follow file numbers; Appendix M (debugging) is a tutorial; `en/17_architecture.md` is an unlisted draft.

Defects found in passing: ch 1 lists `SUB` as a language word (not in `language/0.6/keywords.md`); spec links pinned to `v0.6.8` and statements about "the 0.6.8 toolchain" while the release is 0.6.10; typos in the preface ("maily", "lerning", "junst", "configurating") and ch 5 ("YOu", "`AS`to").

---

## Target structure

Decision (Carlos, 2026-10-27): modules come before classes, right after the procedural core, so `IMPORT` of `BNMath` and `HOST` is never a forward reference.

**Part I — Getting started**

* Preface — absorbs the language classification and application domains from ch 1, shortened.
* 1 Getting Started — installing (absorbs `01a_Installing.md`), first program, `bni check` / `bni run` / `bnc`, reading a diagnostic, editors. No modules, no ARC.

**Part II — The procedural language**

* 2 Values, Types and Variables — comments, `LET`, `CONST`, primitive types, scalar literals, `TYPEOF`, `PRINT`.
* 3 Operators and Conversions — arithmetic, the three divisions, comparison, logical and bitwise, `++`/`--`, `AS`, `AS STRING`.
* 4 Control Flow — `IF`, single-line `IF`, `WHILE`, `REPEAT`, counted `FOR`, loop scope, `EXIT`/`CONTINUE`, `STOP`; every example inside `Start`.
* 5 Functions — declaration, parameters are constant, `VOID`, guard clauses, return analysis (merged, including loops), `Start AS INTEGER` versus `STOP`, function values.
* 6 Vectors, Strings and Structs — vectors, `FOR EACH`, `LEN`, `SIZEOF`, string indexing, `STRUCT`, positional literals, destructuring, value semantics.
* 7 Absence, Alternatives and Errors — `OR`, `NULL`/`NA`/`EOF`, `IS` and every narrowing rule in one place, numeric literals in alternatives, `INPUT`, `Error` (received from a fallible call such as `ASC("")`; a program cannot construct one, `language/0.6/error.md`).
* 8 Modules — `IMPORT`/`EXPORT` of functions, constants and structs, the module search path (one version, checked against `docs/architecture/module-path.md` and the toolchain), cycles, importing `BN*` modules and `HOST` capabilities.

**Part III — Objects and memory**

* 9 Classes and Interfaces — current ch 6 plus "Exporting a class" (from ch 5).
* 10 Memory (ARC) — strong and weak references, release points, `RELEASE`, regions and pointers.

**Part IV — Programs in the real world**

* 11 Standard Library Essentials — `BNMath` (with the numeric limits from ch 2), `BNString`, temporal types (merged), `BNPallet`.
* 12 The HOST — `Args`, `Console` (with positioned output from ch 2), `Clock`, `Random`, `Env`, `Exec`, `NumProcs`.
* 13 Files — `HOST.FileSystem`, one version merged from ch 8 and ch 9.
* 14 Networking — `HOST.Net`, introduction to `BNWeb`.
* 15 Concurrency — `BNDispatch` tutorial (from ch 9 and Appendix J).
* 16 Debugging and Profiling — from Appendix M.

**Appendices (reference), letters in file order**

* A Keywords · B Diagnostics catalogue (consolidates the per-chapter "Diagnostics Introduced" tables) · C Grammar · D `bni` and `bnc` · E External module conventions · F `BNJson` · G `BNLog` · H `BNWeb` · I `BNData` · J `BNSqlite` · K `BNCrypto` · L `BNFfi` · M `BNDispatch` (the module stays; the appendix keeps its reference material, the tutorial moves to ch 15).

---

## Activities

### A — Baseline

- [X] A.0 Rewrite `scripts/check-examples.sh` in shell, with no Python (Carlos, 2026-10-27; project rule: no Python tooling). It keeps the current contract: every ` ```basic ` block that defines `FUNCTION Start` goes through `bni check --module-path "$BN_STDLIB"`; the annotations `bn-check: expect-error`, `bn-check: see examples/DIR` and `bn-check: skip REASON` on the line before the fence; every `examples/*/main.bn` checked with its own directory on the module path; blocks without `Start` counted as fragments; a summary of ok/fail/skipped/fragments and a non-zero exit on any failure. Runs on macOS (BSD tools) and Linux. Evidence: the script is bash + awk only and implements the same rules (the three annotations, the examples directories, the summary line, non-zero exit on failure); the Python version was not run for comparison (no-Python rule).
- [X] A.1 Run the rewritten `scripts/check-examples.sh` against the current `bni` and record the pass/fail/skip counts as the baseline; every later activity must not lower the pass count. Evidence: before the reorganization, 69 OK and 2 failed (BNCrypto) with the working-tree `modules/bn`; both failures came from uncommitted 0.6.11 changes to `BNCrypto.bn`, not from the book. The reference from then on is the v0.6.10 release `bni` with `modules/bn` of tag `v0.6.10`, as the book CI uses.
- [X] A.2 Fix the defects found in passing (`SUB`, typos) before moving text, so the moves carry correct text. Evidence: `SUB` removed with the old classification section; preface and chapter 5 typos fixed; the preface no longer says there is no C interface (BNFfi exists).
- [X] A.3 Verify every "0.6.8 toolchain does not yet …" statement against the current `bni` (notably `RESERVED_IDENTIFIER`); update or remove each; repin spec links to the current release tag. Evidence: `LET for AS INTEGER` and `LET string AS STRING` still compile on 0.6.10, so the statements now name 0.6.10; every link repinned from `v0.6.8` to `v0.6.10` (all targets exist at the tag); book CI `BN_VERSION` set to `v0.6.10`.

### B — File layout

- [X] B.1 Rename the chapter files to the target order with `git mv` (`NN_name.md` for chapters, `appendix_X_name.md` for appendices) in one commit, before content moves, so history follows each file. Evidence: `git mv` of 20 files (history preserved; nothing committed). Exception: Appendix A keeps `en/21_keywords.md`, because the Vim and VS Code extensions download it from that URL; renaming it would break installed plugins.
- [X] B.2 Remove `en/17_architecture.md` from the book (move its filesystem-policy section to `docs/architecture/` if it is not already covered there). Evidence: removed with `git rm`; its only written section (filesystem sandbox) moved to chapter 13 as user-facing text, verified with `bni` and `bnc`.
- [X] B.3 Rewrite `en/toc.md` with the four parts and the appendices; rewrite `10_appendices.md` as the appendix index without the "file numbers are historical" note. Evidence: `en/toc.md` regenerated from the chapter headings, four parts plus appendices; `appendices.md` rewritten as the A–M index.

### C — Part I

- [X] C.1 Ch 1: merge `01a_Installing.md`; move classification and application domains to the preface; remove the ARC, `BN*` list and modules sections (their content lives in ch 8, 10, 11). Evidence: `01a_Installing.md` merged and deleted; install scripts, first program, check/run/compile, conventions, commands (with `bnr`), diagnostic reading, editors; every command output re-run on 0.6.10.
- [X] C.2 Preface: update "How This Book Is Organized" to the four parts. Evidence: preface section "How This Book Is Organized" lists the four parts; new section "Where the Language Fits" holds the condensed classification and domains.

### D — Part II (largest set of forward references)

- [X] D.1 Split current ch 2 into ch 2 (values, types, variables) and ch 3 (operators, conversions); move `++`/`--` from Control Flow into ch 3; move alternative-type, `NULL`/`NA`/`EOF`, vector/`STRUCT` literal, temporal literal, numeric limits, `INPUT` and positioned-output sections to their target chapters. Evidence: `02_values_types_and_variables.md` and new `03_operators_and_conversions.md` (with compound assignment and `++`/`--`, verified on `bni` and `bnc`).
- [X] D.2 Ch 4 Control Flow: remove `IS` narrowing, guard clauses, return analysis and `FOR EACH`; rewrite the examples that call user functions or index vectors so they use only chapters 2–3; keep the worked example only if it fits, otherwise move it to ch 6. Evidence: chapter 4 examples all inside `Start`, new REPEAT, CONTINUE, nested-EXIT and Collatz examples run on `bni` and `bnc`.
- [X] D.3 Ch 5 Functions: merge return analysis from current ch 3 and ch 5, guard clauses from ch 3, constant parameters from ch 7. Evidence: `05_functions.md`: declarations, recursion, constant parameters, guard clauses, return analysis, exit status of `Start`, function values; each example run on both backends.
- [X] D.4 Ch 6 Vectors, Strings and Structs: from current ch 4 plus `FOR EACH`, `LEN`, `SIZEOF` (from ch 8) and the vector/`STRUCT` literal rules (from ch 2). Evidence: `06_vectors_strings_and_structs.md`, including FOR EACH, LEN/SIZEOF, value semantics, and the readings worked example (moved from old ch 3).
- [X] D.5 Ch 7 Absence, Alternatives and Errors: one narrowing section built from ch 2, 3 and 4; `INPUT` as the motivating example; replace the `FS.Open` error example with one that needs no `HOST` (`ASC("")` or another fallible built-in confirmed in the spec). Evidence: new `07_alternatives_and_errors.md`; the error example uses `ASC("")` and a user function that returns the received `Error`; run on both backends.
- [X] D.6 Ch 8 Modules: from current ch 5 and the `EXPORT CONST` example of ch 2; exported classes move out to ch 9; reconcile the search path with Appendix E and the toolchain; `BN*` and `HOST` imports shown with `BNMath` only. Evidence: new `08_modules.md`; the search path was checked against `language/0.6/0.6.md` (5 layers) and exercised (`modules/`, `config.toml`, standard library); examples renamed to `examples/ch08-*` and run.
- [X] D.7 Check every snippet of chapters 2–8 against the rule: no construct before its chapter. Record the check (chapter, construct, first use) in the activity's evidence. Evidence: scan of the code blocks of chapters 2–8 for CLASS/NEW, IMPORT, IS and alternatives, vectors/STRUCT, user functions, FOR EACH before their chapter: only `IS NAN` in chapter 2, a floating-point test explained there.

### E — Part III

- [X] E.1 Ch 9: current ch 6 plus "Exporting a class" and `PRIVATE_ACCESS` from ch 5; the destructor paragraph points forward to ch 10 without using `RELEASE`. Evidence: `09_classes_and_interfaces.md` with a complete Account example, "Exporting a Class" (`examples/ch09-exported-class`), diagnostics and summary; the sentence that said overriding is automatic was corrected (`OVERRIDE` is required).
- [X] E.2 Ch 10: current ch 7 minus constant parameters; the internal representation of alternatives is removed (it is spec material, `language/0.6/0.6.md`) or reduced to one sentence. Evidence: `10_memory.md`: "Parameters keep the argument alive" links to chapter 5; the representation of alternatives removed; regions introduced before pointers; diagnostics and summary.

### F — Part IV

- [X] F.1 Ch 11: `BNMath`, numeric limits, `BNString`, temporal types (merged from ch 2, ch 8 and Appendix A), `BNPallet`. Evidence: new `11_standard_library.md`; the temporal example (Parse, FromLocal, Format, AtZone, Offset) run on both backends.
- [X] F.2 Ch 12: `HOST` capabilities except `FileSystem` and `Net`, with positioned output. Evidence: `12_host.md`; `HOST.Console` import is optional (`console.md`), so the text no longer says it is required.
- [X] F.3 Ch 13: one `HOST.FileSystem` text merged from ch 8 and ch 9. Evidence: new `13_files.md`; the two file examples and the sandbox example run on `bni` and `bnc`. The old chapter-9 file text was dropped: it repeated chapter 8 and its `FS.Exists(...) = TRUE` example does not typecheck.
- [X] F.4 Ch 14 and ch 15: networking and concurrency from ch 9; Appendix M (`BNDispatch`) keeps only reference material. Evidence: `14_networking.md`; new `15_concurrency.md` with ASYNC/AWAIT and Mutex examples run on both backends (the old mutex example used `CreateMutex`/`Acquire`, which do not exist).
- [X] F.5 Ch 16: debugging and profiling from Appendix M. Evidence: `16_debugging_and_profiling.md`, numbered headings removed, unsourced "95%" claim removed, summary added.

### G — Appendices

- [X] G.1 Appendix B: a diagnostics catalogue built from the per-chapter tables, each code linked to the chapter that introduces it; codes checked against the Fluent catalog (`share/bn/diagnostics/`). Evidence: Appendix B lists every code from the per-chapter tables, split into compile-time and run-time, each linked to its chapter; every code was found in `share/bn/diagnostics/` or `crates/bn_diagnostics`.
- [X] G.2 Appendix E: one search-path text, identical to ch 8 or linking to it. Evidence: Appendix E now lists the same 5-layer search path as chapter 8 and links to it.
- [X] G.3 Module appendices relettered F–M; cross-references from chapters updated. Evidence: appendices F–M renamed and retitled; Appendix M rewritten as reference from `language/0.6/bndispatch.md`.

### H — Close

- [X] H.1 Every `← Previous` / `Next →` link and every internal link resolves (scripted check, no Python). Evidence: navigation rewritten from one reading-order list; a link checker (shell) found every relative link and anchor resolved.
- [X] H.2 `scripts/check-examples.sh` passes with at least the baseline count; every snippet that defines `Start` runs on `bni run` with the output the text shows; `bnc` where the text claims it. The `examples/chNN-*` directories are renamed to the new chapter numbers. Evidence: `BNI=<v0.6.10 release bni> BN_STDLIB=<v0.6.10 modules/bn> scripts/check-examples.sh`: 91 OK, 0 failed, 7 annotated, 95 fragments. Every unannotated program run with `bni run` (non-zero exits only where the text expects them); chapters 1–11 and 15 compared `bni` × `bnc`: one difference, a toolchain defect (below).
- [X] H.3 Regenerate `BasicNext_Book.html`. Evidence: regenerated with pandoc 3.9 from `toc.md` and the pages in reading order.
- [ ] H.4 Review the whole book once in reading order with `technical-book-editor`; record findings and fix them before closing.
- [ ] H.5 Move this file to `done/`.

---

## Execution notes (2026-10-27)

H.4 is open: a targeted review ran (stale chapter names and numbers, stale versions, links and anchors, forward references, every complete program executed), but not a line-by-line reading of every page; the module appendices F–L were only renumbered and relinked. H.5 waits for Carlos's review and the commit in the book repository.

Toolchain findings, not fixed here (this execution did not touch BasicNext code). Each still has to be reproduced on a clean `v0.6.10` worktree before it is filed:

* **`bnc` prints a `TIMEZONE` value as the word `TIMEZONE`** (`LET zone AS TIMEZONE` then `PRINT zone`): `bni` prints `UTC` / `America/Sao_Paulo`. Parity defect; seen with `target/release/bnc` 0.6.10.
* **A `bnc --sandbox` binary ignores `BN_FS_POLICY=deny`**: reads and writes under the declared roots still succeed, while `bni run --sandbox` with the same variable denies them, and an unrestricted `bnc` binary honours it. The book (chapter 13) states the specified behaviour. Security defect: the execution policy must narrow the artifact ceiling.
* **`Error.Operation` is empty and `PRINT` of an `Error` omits ` in <Operation>` and `(cause: …)`** for `ASC("")`, on both backends and on the v0.6.10 release `bni` (`Error 1: ASC requires a non-empty STRING`), against `language/0.6/error.md`.
* **The v0.6.10 release `bnc` asset stops with `BUILD_TOOLCHAIN_UNAVAILABLE: libbn_stdlib.a not found`** when used beside the release `libbn_rt` asset; the release ships no `libbn_stdlib.a`. Check whether the installers provide it.
* **Uncommitted 0.6.11 changes to `modules/bn/BNCrypto.bn`** (`Key`, `ImportKey`) are not implemented by the current `bni` build, so the book's BNCrypto examples fail against the working tree but pass against `v0.6.10`. When 0.6.11 ships that API, Appendix K must be updated.
* **A vector parameter accepts element assignment, and the backends disagree on its effect.** The specification (`0.6.md`, "Parameter immutability and place mutation") rejects `v[i] = x` on a value-type parameter with `TYPE_MISMATCH`; the toolchain accepts it. With an open parameter (`v AS INTEGER[]`), `v[0] = 99` leaves the caller's vector unchanged under `bni` and changes it under `bnc` (prints 1 and 99). Chapter 6 teaches the specified rule and warns that 0.6.10 does not enforce it.
* **`UNUSED_BINDING` warns about `STRUCT` fields read only through a destructuring `LET`** (`LET [name, port] = site` still warns that fields `port` and `name` are never read).

Outside the book, one link still points to an old file name and will break once the book is pushed: `basicnext/language/0.6/memory-migration.md` (to `en/07_memory_management.md`, now `en/10_memory.md`). The `docs/releases/*.md` files hold historical links and were left as they are.
