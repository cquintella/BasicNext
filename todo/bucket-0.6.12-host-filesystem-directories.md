# Bucket 0.6.12 — Filesystem Directories, BNString Expansion, and ARC Arrays

**Status:** todo, created 2026-10-27 (Carlos: "isso vira bucket 0.6.12; expandir o BNString... Strings: concatenar com += é quadrático, não há substring nem fatiamento nativos, e literais não aceitam \n nem várias linhas; Arrays: NEW T[n] só aceita tipos numéricos. Não existe STRING[] nem array de objetos, então usei listas encadeadas"). Starts after 0.6.11 closes.
**Source:** Real-world dogfooding in `examples/book_packages.bn` and `todo/proposals/host-filesystem-ops-0.5.3.md`.

Every row is an extension: spec text, red tests on `bni` and `bnc`, shared implementation in `crates/` and `libraries/`, verified across Linux, macOS and Windows.

---

## Part 1: HOST.FileSystem Directory Operations

### Motivation
In Basic Next 0.6.10, `HOST.FileSystem` only manipulates individual regular files (`Open`, `Exists`, `DeleteFile`). Directory operations were explicitly categorized as out of scope in the 0.2/0.6 baseline (`language/0.6/host.md` line 224: *"Outside 0.2: ChangeDirectory, directory create/list/delete, Move, Copy, chmod / chown, Seek, Flush, and a Path object"*).

This part completes the filesystem capability surface, providing full process working directory management, directory lifecycle, listing, and file movement/copying while strictly upholding the fail-closed sandbox policy (`HOST.Policy`).

### API Surface (`HOST.FileSystem`)
Logical import: `IMPORT HOST.FileSystem AS FS`

* `FS.GetCurrentDirectory() AS STRING OR Error`: Returns the absolute canonical current working directory of the process.
* `FS.SetCurrentDirectory(path AS STRING) AS VOID OR Error`: Changes the process working directory to `path`. Fails with `FS.NOT_FOUND` if path does not exist, `FS.INVALID_ARGUMENT` if path is a regular file, `FS.PERMISSION_DENIED` if traversal permission is missing, or `FS.POLICY_DENIED` if outside sandbox roots.
* `FS.CreateDirectory(path AS STRING) AS VOID OR Error`: Creates a directory at `path`.
* `FS.RemoveDirectory(path AS STRING) AS VOID OR Error`: Removes the empty directory at `path`. Fails if the directory is not empty or does not exist.
* `FS.ListDirectory(path AS STRING) AS STRING[] OR Error`: Returns the names of all entries (files, directories, symlinks) within `path`.
* `FS.IsDirectory(path AS STRING) AS BOOLEAN OR Error`: Returns `TRUE` if `path` resolves to an existing directory, `FALSE` if it exists as a non-directory, or `FS.NOT_FOUND` / `Error` if inaccessible.
* `FS.Copy(src, dst AS STRING) AS VOID OR Error`: Copies the regular file from `src` to `dst`.
* `FS.Move(src, dst AS STRING) AS VOID OR Error`: Atomically moves/renames the filesystem entry from `src` to `dst`.

### Sandbox & Execution Policy (`HOST.Policy`)
Under `--sandbox`, any path accessed by `GetCurrentDirectory`, `SetCurrentDirectory`, `CreateDirectory`, `RemoveDirectory`, `ListDirectory`, `Copy` or `Move` must reside strictly below the declared `--read-root` and `--write-root` boundaries, failing closed with `FS.POLICY_DENIED` on boundary escape.

---

## Part 2: BNString Expansion & String Ergonomics

### Motivation
In `examples/book_packages.bn`, strings require dozens of custom helper functions (`Mid`, `SplitOn`, `IndexOf`, `Trim`, `Newline`). String concatenation via `+=` in loops is quadratic O(n²) because each assignment allocates a new buffer and copies prior contents. Furthermore, string literals cannot represent newlines (`\n`) directly or span multiple lines.

### Technical Specification & Design
* **Escape Sequences in String Literals:**
  * Update frontend lexer to decode standard escape sequences inside `"..."`: `\n` (LF, code 10), `\r` (CR, code 13), `\t` (TAB, code 9), `\"` (quote), `\\` (backslash), and `\0` (NUL).
  * Multiline string literals: support triple-quoted strings `"""..."""` preserving line breaks without manual concatenation.
* **Slicing & Substring in `BNString`:**
  * `Substring(text AS STRING, start AS INTEGER, length AS INTEGER) AS STRING`: O(k) substring extraction with bounds clamping (or returning `Error` on negative offsets).
  * `Slice(text AS STRING, start AS INTEGER, finish AS INTEGER) AS STRING`: Python/Rust-like slice from `start` up to `finish`.
  * Core helpers in `BNString`: `IndexOf(text, needle AS STRING) AS INTEGER`, `LastIndexOf(text, needle AS STRING) AS INTEGER`, `StartsWith(text, prefix AS STRING) AS BOOLEAN`, `EndsWith(text, suffix AS STRING) AS BOOLEAN`, `ReplaceAll(text, needle, replacement AS STRING) AS STRING`, `Trim(text AS STRING) AS STRING`, `Split(text, delimiter AS STRING) AS STRING[]`, `Join(elements AS STRING[], separator AS STRING) AS STRING`.
* **Efficient String Concatenation (`StringBuilder`):**
  * `EXPORT CLASS StringBuilder` in `modules/bn/BNString.bn`:
    * `Append(text AS STRING) AS VOID` (amortized O(1) exponential buffer growth)
    * `AppendLine(text AS STRING) AS VOID`
    * `AppendChar(code AS INTEGER) AS VOID`
    * `Length() AS INTEGER`
    * `ToString() AS STRING`
    * `Clear() AS VOID`
  * Zero O(n²) copying during text processing, document generation, and serialization.

---

## Part 3: ARC Arrays (`NEW STRING[n]` and `NEW ClassName[n]`)

### Motivation
Currently, `NEW T[n]` is restricted strictly to numeric primitive types (`BYTE`, `INTEGER`, `FLOAT`, etc.). Attempting to allocate `NEW STRING[n]` or `NEW MyClass[n]` is rejected by semantic analysis with a type restriction error. Consequently, application code must implement linked lists (`StringList`, `Node`) with severe pointer-chasing overhead and lack of random access.

### Technical Specification & Design
* **Semantic Analysis Expansion:**
  * Permit `NEW STRING[count]` and `NEW ClassName[count]` (or `NEW (ClassName OR NULL)[count]`).
  * Initialize `NEW STRING[n]` with empty strings `""` across all slots.
  * Initialize `NEW ClassName[n]` with `NULL` (requiring alternative type `ClassName OR NULL` or default constructor invocation).
* **Runtime ARC Management (`bn_rt` and `bn_interp`):**
  * In `bn_value` and `bn_rt`: region handles for reference types maintain an element type descriptor indicating ARC management.
  * Storing into an index (`array[i] = item`): retains the new reference, releases the prior reference in slot `i`.
  * Destructor / Deallocation (`RELEASE` or scope exit): when the array pointer buffer is freed, iterates through all `n` slots and decrements the ARC count (`release`) of each contained `STRING` or `OBJECT` instance before freeing the underlying pointer buffer. Zero memory leaks guaranteed.
* **Compiler Lowering (`bnc`):**
  * Emits LLVM loops for array initialization and cleanup loops for array destruction invoking `bn_rt_str_release` or `bn_rt_arc_release` on each element.

---

## Activity Checklist

### Part 1: Filesystem Directories
- [ ] **FS-DIR-1 (Spec):** Update `language/0.6/host.md` and companion error codes (`FS.IS_NOT_DIRECTORY`, `FS.DIRECTORY_NOT_EMPTY`).
- [ ] **FS-DIR-2 (C ABI in bn_rt):** Implement native C ABI symbols in `crates/bn_rt/src/file_abi.rs` and `crates/bn_rt/src/file.rs` (`get_current_directory`, `set_current_directory`, `create_directory`, `remove_directory`, `list_directory`, `is_directory`, `copy`, `move`).
- [ ] **FS-DIR-3 (Interpreter Adapter):** Implement matching capability methods in `crates/bn_interp` / `bn_interpret_driver` with sandbox policy re-check.
- [ ] **FS-DIR-4 (Compiler Lowering):** Lowering and native symbols wired in `bnc` (`crates/bn_llvm`).
- [ ] **FS-DIR-5 (Tests & Parity):** Parity test fixtures across `bni` and `bnc` covering directory lifecycle and sandbox containment negatives.

### Part 2: BNString & Ergonomics
- [ ] **STR-1 (Lexer Escapes):** Support `\n`, `\r`, `\t`, `\"`, `\\` and multiline `"""..."""` in frontend lexer.
- [ ] **STR-2 (BNString Members):** Implement `Substring`, `Slice`, `IndexOf`, `LastIndexOf`, `StartsWith`, `EndsWith`, `ReplaceAll`, `Trim`, `Split`, `Join` in `libraries/bn_core_string` and `modules/bn/BNString.bn`.
- [ ] **STR-3 (StringBuilder):** Implement `StringBuilder` class in `libraries/bn_core_string` with amortized O(1) buffer append, exposed in `BNString.bn`.
- [ ] **STR-4 (Tests):** Unit tests and parity tests proving O(n) linear performance for text construction and exact slice/escape behaviour.

### Part 3: ARC Arrays
- [ ] **ARR-1 (Semantic Analysis):** Lift restriction in `bn_frontend` semantic checker to allow `NEW STRING[n]` and `NEW (T OR NULL)[n]`.
- [ ] **ARR-2 (Interpreter Runtime):** Implement ARC element tracking for pointer regions in `crates/bn_interp` / `crates/bn_value`.
- [ ] **ARR-3 (Native Compiler & Runtime):** Implement element initialization and loop destruction in `crates/bn_llvm` and `crates/bn_rt`.
- [ ] **ARR-4 (Heap & Leak Tests):** Parity tests in `tests/heap.rs` verifying that allocating and destroying `NEW STRING[1000]` and `NEW MyClass[1000]` frees 100% of element buffers without memory leaks.
