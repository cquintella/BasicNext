# Bucket 0.6.16 — String Ergonomics, ARC Arrays, and Directory Listing

**Status:** todo. Renumbered from 0.6.13 on 2026-10-28; 0.6.13 stays unassigned. Split from `todo/bucket-0.6.12-host-filesystem-directories.md`. Starts after the current-language lane: 0.6.15, plus 0.6.14b and 0.6.15b when CRY-1 is yes. `ListDirectory` uses the 0.6.12 no-follow walk. This bucket does not block 0.6.14 or 0.6.15. 0.6.17 waits for it (`STRING[]`, `PublicKey[]`). 0.6.9b starts only after this bucket: `STRING[]` is lowered here as today's one-word `STRING` (`bn_rt_str_release`). 0.7.0 waits for this bucket.

Every row is an extension: spec text, red tests on `bni` and `bnc`, shared implementation, verified across Linux, macOS and Windows.

`\"` and `\\` already decode. `\n`, `\r`, `\t`, and `\0` are rejected by the lexer today, so accepting them does not change a program that runs. A string literal still cannot contain a raw line break. `"""..."""` is new syntax.

## Order inside this bucket

`Split`, `Join`, and `ListDirectory` return `STRING[]`. `ARR-1` lands before those three. Escapes and `StringBuilder` do not need an array.

## Part 1: BNString

In `examples/book_packages.bn`, strings require custom helpers (`Mid`, `SplitOn`, `IndexOf`, `Trim`, `Newline`). `+=` in a loop allocates on every append.

* Lexer escapes inside `"..."`: `\n` (LF, code 10), `\r` (CR, code 13), `\t` (TAB, code 9), `\0` (NUL). `\"` and `\\` stay as they are.
* Multiline literals: `"""..."""`, preserving line breaks.
* `Substring(text AS STRING, start AS INTEGER, length AS INTEGER) AS STRING`. A negative offset is `Error`, not a clamp.
* `Slice(text AS STRING, start AS INTEGER, finish AS INTEGER) AS STRING`.
* `IndexOf`, `LastIndexOf`, `StartsWith`, `EndsWith`, `ReplaceAll`, `Trim`.
* `Split(text, delimiter AS STRING) AS STRING[]` and `Join(elements AS STRING[], separator AS STRING) AS STRING`, after `ARR-1`.
* `EXPORT CLASS StringBuilder` in `modules/bn/BNString.bn`: `Append`, `AppendLine`, `AppendChar`, `Length`, `ToString`, `Clear`. Append grows the buffer. It does not copy the whole result on every call.

## Part 2: ARC arrays

`NEW T[n]` accepts numeric primitives. `NEW STRING[n]` and `NEW MyClass[n]` are rejected. Linked lists stay valid; this bucket does not remove them.

* `NEW STRING[count]` is legal. Every slot starts as `""`.
* `NEW (ClassName OR NULL)[count]` is legal. Every slot starts as `NULL`. A class array without the `OR NULL` alternative is rejected: there is no silent default constructor.
* Storing `array[i] = item` retains the new reference and releases the previous one.
* Releasing the array releases every element, then the buffer.
* `bnc` emits the initialization and the release loop. The element is the one-word `STRING` this release uses.

## Part 3: ListDirectory

* `FS.ListDirectory(path AS STRING) AS STRING[] OR Error` returns the names of the entries. It does not return symlink targets. The final component is opened with `O_DIRECTORY | O_NOFOLLOW` (Windows: `FILE_FLAG_OPEN_REPARSE_POINT`, and a reparse point is rejected). Names are sorted. `.` and `..` are not returned.
* The directory must sit inside a `--read-root`, including the root. A write root is not required. Failure is `FS.POLICY_DENIED`.

## Activity checklist

- [ ] **STR-1 (Lexer escapes):** `\n`, `\r`, `\t`, `\0`, and `"""..."""`. Do not change `\"` or `\\`.
- [ ] **ARR-1 (Semantic analysis):** Allow `NEW STRING[n]` and `NEW (T OR NULL)[n]`. Reject `NEW ClassName[n]` without `OR NULL`.
- [ ] **ARR-2 (Interpreter):** ARC element tracking for those regions in `crates/bn_interp` / `crates/bn_value`.
- [ ] **ARR-3 (Native):** Initialization and release loops in `crates/bn_llvm` and `crates/bn_rt`, calling `bn_rt_str_release` or `bn_rt_arc_release`.
- [ ] **ARR-4 (Tests):** `NEW STRING[1000]` and `NEW (MyClass OR NULL)[1000]` release every element on both backends.
- [ ] **STR-2 (Members):** `Substring`, `Slice`, `IndexOf`, `LastIndexOf`, `StartsWith`, `EndsWith`, `ReplaceAll`, `Trim`, `Split`, `Join`.
- [ ] **STR-3 (StringBuilder):** Amortized append, exposed in `BNString.bn`.
- [ ] **STR-4 (Tests):** Escape fixtures and a linear append. Both backends.
- [ ] **FS-LIST-1:** `ListDirectory` on the 0.6.12 no-follow walk. `STRING[]` from `ARR-1`. Sorted names. Read root only. Symlink directory is not listed. Both backends.
