# Bucket 0.6.12 — Filesystem Directories and Private Create

**Status:** todo, created 2026-10-27. Starts after 0.6.11 closes. On 2026-10-28 the string escapes, `StringBuilder`, `STRING[]`, and object arrays moved to `todo/bucket-0.6.16-string-and-arrays.md`, and `ListDirectory` moved with them: this bucket does not return `STRING[]`. 0.6.14 starts after this bucket closes and does not wait for 0.6.16. 0.6.13 is unassigned.
**Source:** Real-world dogfooding in `examples/book_packages.bn` and `todo/proposals/host-filesystem-ops-0.5.3.md`.

Every row is an extension: spec text, red tests on `bni` and `bnc`, shared implementation in `crates/` and `libraries/`, verified across Linux, macOS and Windows.

---

## Part 1: HOST.FileSystem Directory Operations

### Motivation
In Basic Next 0.6.10, `HOST.FileSystem` only manipulates individual regular files (`Open`, `Exists`, `DeleteFile`). Directory operations were explicitly categorized as out of scope in the 0.2/0.6 baseline (`language/0.6/host.md` line 224: *"Outside 0.2: ChangeDirectory, directory create/list/delete, Move, Copy, chmod / chown, Seek, Flush, and a Path object"*).

This part adds working-directory management, directory create and remove, copy, move, and an owner-only create. Listing waits for `STRING[]` in 0.6.16. The fail-closed sandbox policy (`HOST.Policy`) stays.

### API Surface (`HOST.FileSystem`)
Logical import: `IMPORT HOST.FileSystem AS FS`

* `FS.GetCurrentDirectory() AS STRING OR Error`: Returns the absolute canonical current working directory of the process.
* `FS.SetCurrentDirectory(path AS STRING) AS VOID OR Error`: Changes the process working directory to `path`. Fails with `FS.NOT_FOUND` if path does not exist, `FS.INVALID_ARGUMENT` if path is a regular file, `FS.PERMISSION_DENIED` if traversal permission is missing, or `FS.POLICY_DENIED` if outside sandbox roots.
* `FS.CreateDirectory(path AS STRING) AS VOID OR Error`: Creates one directory at `path`. The parent must already exist. An existing path is `FS.ALREADY_EXISTS` and is not chmod'd.
* `FS.OpenNew(path AS STRING) AS FS.File OR Error`: Creates a new regular file and returns it open for writing. The parent must already exist. An existing path, including a symlink, is `FS.ALREADY_EXISTS`. The call does not truncate and does not follow. This is the owner-only create. `FS.Copy` is not a substitute: `Copy` needs a source file.
* `FS.RemoveDirectory(path AS STRING) AS VOID OR Error`: Removes the empty directory at `path`. Fails if the directory is not empty or does not exist. A symlink is not removed and its target is not removed.
* `FS.IsDirectory(path AS STRING) AS BOOLEAN OR Error`: `TRUE` only when `path` is a directory without following a symlink. A symlink is `FALSE`. Missing is `FS.NOT_FOUND`.
* `FS.Copy(src, dst AS STRING) AS VOID OR Error`: Copies one regular file. An existing destination is `FS.ALREADY_EXISTS`.
* `FS.Move(src, dst AS STRING) AS VOID OR Error`: Renames one entry on the same filesystem. Replaces an existing regular file. Does not replace a directory or a symlink.

### Sandbox & Execution Policy (`HOST.Policy`)
Read operations (`GetCurrentDirectory`, `IsDirectory`, and the source of `Copy`) must sit inside a `--read-root`, including the root itself. Write operations (`SetCurrentDirectory` is not a write; `CreateDirectory`, `OpenNew`, `RemoveDirectory`, the destination of `Copy`, and both paths of `Move`) must sit inside a `--write-root`. A read does not also require a write root. `SetCurrentDirectory` requires the directory to sit inside a read root. Failure is `FS.POLICY_DENIED`. The check walks from the pinned root with `openat` / the Windows reparse-point walk. It does not trust `canonicalize` of a symlink.

### Permissions (2026-10-28)

Reviewed against `crates/bn_rt/src/secure_fs.rs`, `secure_fs_windows.rs`, and `policy.rs`. `chmod` / `chown` stay outside this bucket, as `host.md` already says. No `SetMode`. A mode argument that can set setuid, setgid, sticky, or world write is not added.

What creation does today. Rooted `FS.Open` for `WRITE` and `APPEND` calls `openat` with mode `0o666`. The kernel masks that with the process umask, so umask `022` yields `0644` and umask `0` yields `0666`. The unrestricted branch uses `OpenOptions` and the same default. Windows create inherits the parent DACL and opens with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`. Nothing in the current module forces owner-only. This bucket does not change `FS.Open`. Programs that already create files keep that result. Changing it is a BDFL gate, not an activity here.

New files and directories from this bucket are owner-only, and the umask is not the policy:

* `CreateDirectory` is mode `0700` on Unix. After the create, the directory fd is `fchmod`'d to `0700`. If the resulting mode has any group or other bit set, including a bit forced by a default ACL, the directory is removed and the call fails. Windows sets an owner-only DACL for the current user. If that DACL cannot be set, the directory is removed and the call fails. No `create_dir_all`.
* `OpenNew` creates the file at mode `0600` with the same check. Failure removes the file. The returned `FS.File` is open for writing. A later `FS.Open` `WRITE` of that path truncates and keeps the mode; it is not how the file is created.
* `Copy` creates the destination at mode `0600` with the same check and the same failure cleanup. It does not copy the source mode, owner, setuid, setgid, sticky bit, or ACL. It does not call `std::fs::copy`. Source and destination must be regular files opened without following a symlink. A symlink at either path fails closed and nothing is written through it.
* `Move` is `rename` / `renameat` of the directory entry. It preserves the inode, so it preserves mode and owner. It is not a copy. `EXDEV` is `FS.IO_FAILED` with no fallback. The destination symlink is not followed and is not replaced. Replacing a regular file is the POSIX replace that `DownloadReplace` in bucket 0.6.15 needs. A private file stays private only when the source inode was private. Callers that need a private result create the source with the owner-only operation, then `Move`.
* `RemoveDirectory` is `unlinkat(..., AT_REMOVEDIR)` or the Windows equivalent on the entry itself. Non-empty is `FS.DIRECTORY_NOT_EMPTY`. A symlink is `FS.IS_NOT_DIRECTORY`.
* `IsDirectory` and `OpenNew` open the final component with `O_NOFOLLOW` (Windows: `FILE_FLAG_OPEN_REPARSE_POINT` and reject a reparse point). `ListDirectory` is 0.6.16 and uses the same walk.
* `SetCurrentDirectory` opens that directory the same way and `fchdir`s the fd. It does not call `set_current_dir` on a path string. The working directory is process-wide: every task sees it. Relative paths still pass the sandbox check after they are made absolute. `GetCurrentDirectory` returns the path of that fd. When a sandbox is in force and that path is outside every read root, the result is `FS.POLICY_DENIED`.

`FS.Open` remains the shared create for ordinary output. Cookie files, download temporaries, and ciphertext or plaintext from buckets 0.6.14 and 0.6.15 call `FS.OpenNew`. They do not use `FS.Open` `WRITE` and they do not chmod afterwards.

Residuals, no change in this bucket. `FS.Open` stays `0666` masked by umask, and on Windows it keeps the share mode and the inherited DACL. A parent default ACL that this platform cannot strip fails the new create rather than being left in place. Group ownership stays the creating process's group. No chown.

---

String escapes, `StringBuilder`, `NEW STRING[n]`, `NEW ClassName[n]`, and `FS.ListDirectory` are `todo/bucket-0.6.16-string-and-arrays.md`. They are not activities of this bucket.

## Activity Checklist

### Part 1: Filesystem Directories
- [ ] **FS-DIR-1 (Spec):** Update `language/0.6/host.md` and companion error codes (`FS.IS_NOT_DIRECTORY`, `FS.DIRECTORY_NOT_EMPTY`, `FS.ALREADY_EXISTS`). `chmod` / `chown` stay outside. Do not change the `FS.Open` create mode in that text. `OpenNew` is in that text.
- [ ] **FS-DIR-2 (C ABI in bn_rt):** Implement native C ABI symbols in `crates/bn_rt/src/file_abi.rs` and `crates/bn_rt/src/file.rs` (`get_current_directory`, `set_current_directory`, `create_directory`, `remove_directory`, `is_directory`, `copy`, `move`, `open_new`). No `list_directory` here.
- [ ] **FS-DIR-3 (Interpreter Adapter):** Implement matching capability methods in `crates/bn_interp` / `bn_interpret_driver` with sandbox policy re-check.
- [ ] **FS-DIR-4 (Compiler Lowering):** Lowering and native symbols wired in `bnc` (`crates/bn_llvm`).
- [ ] **FS-DIR-5 (Tests & Parity):** Parity test fixtures across `bni` and `bnc` covering directory lifecycle and sandbox containment negatives.
- [ ] **FS-PERM-1 (Owner-only create):** `CreateDirectory` is `0700`. `OpenNew` and the destination of `Copy` are `0600`. Neither copies mode, owner, or setuid. A result with group or other bits, or a Windows DACL that is not owner-only, is deleted and the call fails. Umask `0` does not widen either mode. `FS.Open` is not modified.
- [ ] **FS-PERM-2 (No follow, no copy-rename):** `IsDirectory`, `OpenNew`, `SetCurrentDirectory`, `Copy`, and `RemoveDirectory` do not follow a symlink. `Move` is same-filesystem rename, replaces a regular file, and returns `IO_FAILED` on `EXDEV` without copying. A symlink destination is not replaced.
- [ ] **FS-PERM-3 (Roots):** A read checks read roots only. A write checks write roots only. `SetCurrentDirectory` uses `fchdir` on the checked directory fd. `GetCurrentDirectory` outside a sandboxed read root is `POLICY_DENIED`.
- [ ] **FS-PERM-4 (Consumers):** `FS.OpenNew` is what buckets 0.6.14 and 0.6.15 call for ciphertext, plaintext, cookie files, and download temporaries. Those calls do not go through `FS.Open` `WRITE` and do not go through `FS.Copy`.
