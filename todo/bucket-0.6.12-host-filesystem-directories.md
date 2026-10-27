# Bucket 0.6.12 — HOST.FileSystem Directory Operations and Path Manipulation

**Status:** todo, created 2026-10-27 (Carlos: "isso vira bucket 0.6.12"). Starts after 0.6.11 closes.
**Source:** Decision 2026-10-27, advancing `todo/proposals/host-filesystem-ops-0.5.3.md`.

Every row is an extension to the built-in host capability `HOST.FileSystem`: spec text, red tests on `bni` and `bnc`, shared implementation in `crates/bn_rt` and `bn_interp`, verified across Linux, macOS and Windows.

---

## Motivation

In Basic Next 0.6.10, `HOST.FileSystem` only manipulates individual regular files (`Open`, `Exists`, `DeleteFile`). Directory operations were explicitly categorized as out of scope in the 0.2/0.6 baseline (`language/0.6/host.md` line 224: *"Outside 0.2: ChangeDirectory, directory create/list/delete, Move, Copy, chmod / chown, Seek, Flush, and a Path object"*).

This bucket completes the filesystem capability surface, providing full process working directory management, directory lifecycle, listing, and file movement/copying while strictly upholding the fail-closed sandbox policy (`HOST.Policy`).

---

## API Surface (`HOST.FileSystem`)

Logical import: `IMPORT HOST.FileSystem AS FS`

### Working Directory
* `FS.GetCurrentDirectory() AS STRING OR Error`: Returns the absolute canonical current working directory of the process.
* `FS.SetCurrentDirectory(path AS STRING) AS VOID OR Error`: Changes the process working directory to `path`. Fails with `FS.NOT_FOUND` if path does not exist, `FS.INVALID_ARGUMENT` if path is a regular file, `FS.PERMISSION_DENIED` if traversal permission is missing, or `FS.POLICY_DENIED` if outside sandbox roots.

### Directory Lifecycle & Inspection
* `FS.CreateDirectory(path AS STRING) AS VOID OR Error`: Creates a directory at `path`.
* `FS.RemoveDirectory(path AS STRING) AS VOID OR Error`: Removes the empty directory at `path`. Fails if the directory is not empty or does not exist.
* `FS.ListDirectory(path AS STRING) AS STRING[] OR Error`: Returns the names of all entries (files, directories, symlinks) within `path`.
* `FS.IsDirectory(path AS STRING) AS BOOLEAN OR Error`: Returns `TRUE` if `path` resolves to an existing directory, `FALSE` if it exists as a non-directory, or `FS.NOT_FOUND` / `Error` if inaccessible.

### File Operations
* `FS.Copy(src, dst AS STRING) AS VOID OR Error`: Copies the regular file from `src` to `dst`.
* `FS.Move(src, dst AS STRING) AS VOID OR Error`: Atomically moves/renames the filesystem entry from `src` to `dst`.

---

## Sandbox & Execution Policy (`HOST.Policy`)

Every new method enforces `HOST.Policy` validation:
* Under `--sandbox`, any path accessed by `GetCurrentDirectory`, `SetCurrentDirectory`, `CreateDirectory`, `RemoveDirectory`, `ListDirectory`, `Copy` or `Move` must reside strictly below the declared `--read-root` and `--write-root` boundaries.
* Attempting to navigate outside sandbox roots via `SetCurrentDirectory` or reading outside via `ListDirectory` fails closed with `FS.POLICY_DENIED`.

---

## Activity Checklist

- [ ] **FS-DIR-1 (Spec):** Update `language/0.6/host.md` and companion error codes (`FS.IS_NOT_DIRECTORY`, `FS.DIRECTORY_NOT_EMPTY`).
- [ ] **FS-DIR-2 (C ABI in bn_rt):** Implement native C ABI symbols in `crates/bn_rt/src/file_abi.rs` and `crates/bn_rt/src/file.rs`:
  - `bn_rt_fs_get_current_directory`
  - `bn_rt_fs_set_current_directory`
  - `bn_rt_fs_create_directory`
  - `bn_rt_fs_remove_directory`
  - `bn_rt_fs_list_directory`
  - `bn_rt_fs_is_directory`
  - `bn_rt_fs_copy`
  - `bn_rt_fs_move`
- [ ] **FS-DIR-3 (Interpreter Adapter):** Implement matching capability methods in `crates/bn_interp` / `bn_interpret_driver` with sandbox policy re-check.
- [ ] **FS-DIR-4 (Compiler Lowering):** Lowering and native symbols wired in `bnc` (`crates/bn_llvm`).
- [ ] **FS-DIR-5 (Tests & Parity):** Parity test fixtures across `bni` and `bnc` covering directory creation, entry listing, removal, working directory navigation, and sandbox containment negatives.
- [ ] **FS-DIR-6 (Documentation & Book):** Update tutorial book (`docs/book/en/08_standard_library_and_host.md`, `09_io_and_concurrency.md`) and man pages.
