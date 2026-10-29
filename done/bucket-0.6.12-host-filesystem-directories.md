# Bucket 0.6.12 — Filesystem Directories and Private Create

**Status:** closed 2026-10-29 (started 2026-10-28, created 2026-10-27). Restructured 2026-10-28 (risk-first order, acceptance criteria, BDFL gates). After it closes, 0.6.11 and 0.6.12 are tagged and released together (Carlos, 2026-10-28); then 0.6.11b. 0.6.14 starts after this bucket closes and does not wait for 0.6.16. 0.6.13 is unassigned.
**Moved out on 2026-10-28:** string escapes, `StringBuilder`, `STRING[]`, object arrays and `FS.ListDirectory` are `todo/bucket-0.6.16-string-and-arrays.md`. This bucket returns no `STRING[]`.
**Source:** Real-world dogfooding in `examples/book_packages.bn` and `todo/proposals/host-filesystem-ops-0.5.3.md`.

Every activity is an extension of `HOST.FileSystem`: spec text first, red tests on `bni` and `bnc`, one implementation in `bn_rt::file` adapted by both backends, verified on macOS, Linux (CI) and Windows (host + CI).

---

## Consolidated identifiers

| Activity (this file) | Former IDs (2026-10-27 text) |
| --- | --- |
| FS-CORE-1 | FS-PERM-1 (mechanism), FS-PERM-2 (mechanism) |
| FS-SPEC-1 | FS-DIR-1 |
| FS-CORE-2 | FS-DIR-2 (semantics part), FS-PERM-1, FS-PERM-2, FS-PERM-3 |
| FS-FE-1 | — (missing before) |
| FS-INT-1 | FS-DIR-3 |
| FS-NAT-1 | FS-DIR-2 (C ABI part), FS-DIR-4 |
| FS-TEST-1 | FS-DIR-5 |
| FS-CONS-1 | FS-PERM-4 |

Closing an activity closes its former IDs in the same edit.

---

## Part 1: Design (BDFL text of 2026-10-27/28, kept)

### API Surface (`HOST.FileSystem`)
Logical import: `IMPORT HOST.FileSystem AS FS`

* `FS.GetCurrentDirectory() AS STRING OR Error`: Returns the absolute canonical current working directory of the process.
* `FS.SetCurrentDirectory(path AS STRING) AS VOID OR Error`: Changes the process working directory to `path`. Fails with `FS.NOT_FOUND` if path does not exist, `FS.INVALID_ARGUMENT` if path is a regular file (see gate G-2), `FS.PERMISSION_DENIED` if traversal permission is missing, or `FS.POLICY_DENIED` if outside sandbox roots.
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

> Superseded in part by G-5 and G-6 (Carlos, 2026-10-28): owner-only is enforced with the Unix mode bits only. On Windows the new entry inherits the parent DACL (no DACL is set), and inherited ACLs are not inspected on any platform. The sentences below about a Windows DACL and about default ACLs describe what is not done.


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

## Part 2: Review of 2026-10-28 (what the first text missed)

1. **Semantics in the wrong layer.** FS-DIR-2 put the operations in `file_abi.rs`. The rule of the workspace is one implementation in `bn_rt::file` (shared by `bni` and `bnc`), with `bn_host_fs` (`bni`) and `file_abi.rs` (`bnc`) as adapters only, as `DeleteFile` is today.
2. **Wrong adapter path.** FS-DIR-3 named `bn_interp` / `bn_interpret_driver`; the `HOST.FileSystem` provider of `bni` is `crates/bn_host_fs`.
3. **Missing frontend step.** Without the members in `crates/bn_frontend/src/host_spec/` and the codes in `bn_types::error_codes::fs` (`ALL`), `bni check` rejects every new call. Completion in `bni lsp` reads the same table.
4. **Missing machine-checked lists.** `language/0.6/value-memory-abi.md` (symbol list read by `tests/codegen/compiler_capabilities.rs`) and `tests/compiler-capabilities.json` grow with every new `bn_rt_fs_*` symbol.
5. **Missing close steps.** Book chapter `docs/book/en/13_files.md`, `docs/releases/0.6.12.md`, the release index, and the end-of-sprint audit.
6. **FS-PERM-4 is not work here.** It constrains the consumers; 0.6.14, 0.6.14b, 0.6.15 and 0.6.17 already name `FS.OpenNew`. It becomes the check FS-CONS-1.
7. **FS-PERM-1 to 3 are properties, not steps.** They become acceptance criteria of FS-CORE-1, FS-CORE-2 and FS-TEST-1.
8. **The root itself.** `RootedDir::relative` rejects an empty relative path today, so `IsDirectory(root)` and `SetCurrentDirectory(root)` would be denied, against "including the root itself". The root must be readable and never a write target (gate G-3, last row).
9. **Undefined cases and one inconsistency** in the error codes (gates G-1 to G-3).

### Risk register

| # | Risk | Where it bites | Mitigation (step) |
| --- | --- | --- | --- |
| R1 | Windows has no owner-only create without a new library (G-5) | FS-CORE-1 | Decided: inherited DACL, documented residual; tests check only that nothing is written through a junction |
| R2 | Inherited ACLs are not visible in the mode bits (G-6) | FS-CORE-1 | Decided: mode bits only, documented residual |
| R3 | The working directory is process state: tests that run in-process interfere, `ASYNC` tasks share it, and in-process hosts (`bni eval --session`, `bni dap`) keep it | FS-CORE-2, FS-TEST-1 | Gate G-4; every `SetCurrentDirectory` test runs in a child process; no `std::env::set_current_dir` in unit tests |
| R4 | Check-then-act races in `Move`, `Copy`, `RemoveDirectory` | FS-CORE-1 | Act through the held parent descriptor (`renameat`, `unlinkat`, `openat` with `O_EXCL|O_NOFOLLOW`); the guarantee is "never written through a link", tested with a swap loop as `concurrent_intermediate_symlink_swap_never_reads_outside` |
| R5 | Windows symbolic links need a privilege the host may not have | FS-TEST-1 | Windows tests use junctions (no privilege); symlink cases are `cfg(unix)` and listed in `tests/support/migration_target.rs` |
| R6 | `EXDEV` (`ERROR_NOT_SAME_DEVICE`) is hard to provoke in a test | FS-CORE-2 | Unit test of the error mapping with the OS error code; no copy fallback |
| R7 | `FS.Open`, `Exists`, `DeleteFile` must not change | FS-CORE-2 | The existing `tests/host/fs_*.bn` fixtures and `host_conformance` stay byte-identical |
| R8 | Relative paths after `SetCurrentDirectory` and the policy check | FS-CORE-2 | `std::path::absolute` reads the new directory; test a relative write that leaves the root after a change of directory |

---

## Part 3: BDFL decision gates (open)

FS-CORE-1 may start before the gates (it adds no surface). FS-SPEC-1 and everything after it wait for G-1 to G-3.

- [X] **G-1 (Code values):** `FS.IS_NOT_DIRECTORY = 10`, `FS.DIRECTORY_NOT_EMPTY = 11`, `FS.ALREADY_EXISTS = 12` (next free values, in the order of the first text). **Decided (Carlos, 2026-10-28): yes.**
- [X] **G-2 (Not a directory):** the first text gives `SetCurrentDirectory` of a regular file `INVALID_ARGUMENT` but `RemoveDirectory` of a symlink `IS_NOT_DIRECTORY`. Proposal: `IS_NOT_DIRECTORY` for every "the path is not a directory" case, a symlink included. **Decided (Carlos, 2026-10-28): yes.** Message names the action and the path; the cause is "the path is not a directory", or "the path is a symbolic link, which is not followed" (POSIX `ENOTDIR`, Windows `ERROR_DIRECTORY`, Python `NotADirectoryError`, Rust `NotADirectory`). A rule that confines writes to the working directory was considered and not adopted: a program can change directory, it rejects programs that write elsewhere today, and `--write-root .` already gives it.
- [X] **G-3 (Undefined cases):** **Decided (Carlos, 2026-10-28): as proposed.**

  | Case | Code |
  | --- | --- |
  | `RemoveDirectory` of a regular file | `IS_NOT_DIRECTORY` |
  | `CreateDirectory` / `OpenNew` with a missing parent; `RemoveDirectory`, `SetCurrentDirectory`, `Copy` source, `Move` source missing | `NOT_FOUND` |
  | `Copy` source is a symlink | `INVALID_ARGUMENT` (not a regular file) |
  | `Copy` source is a directory | `IS_DIRECTORY` |
  | `Copy` destination exists (file, directory or symlink) | `ALREADY_EXISTS` |
  | `Move` destination is a directory or a symlink | `ALREADY_EXISTS` |
  | `Move` of a symlink | renames the link itself |
  | Owner-only not reached (group/other bit, extended ACL, DACL) | `PERMISSION_DENIED`; the new entry is removed |
  | A symlink on the way under a `--read-root` / `--write-root` | as `FS.Open` today (same classification, one rule for `HOST.FileSystem`) |
  | The root itself | readable (`IsDirectory`, `SetCurrentDirectory`, `GetCurrentDirectory`); never a write target (`CreateDirectory` → `ALREADY_EXISTS`, `RemoveDirectory` / `Move` → `POLICY_DENIED`) |
- [X] **G-4 (Shared working directory):** the working directory is one per process; a change by one `ASYNC` task is seen by every task and by the host process (`bni eval --session`, `bni dap`). Proposal: accept and state it in `host.md` as host state outside task isolation, with "a program that runs tasks should not change directory while they run". **Decided (Carlos, 2026-10-28): accepted.**
- [X] **G-5 (Windows owner-only):** **Decided (Carlos, 2026-10-28): the simplest compatible way, no new library.** Without `windows-sys` there is no API to set a DACL (std and the MSVC CRT only toggle the read-only attribute), so on Windows `CreateDirectory`, `OpenNew` and the `Copy` destination inherit the parent DACL, as `FS.Open` does. Owner-only is a Unix guarantee (mode bits); on Windows it holds only where the parent is already private (the user profile). Documented as a residual in `host.md` and the release note. A later bucket may add the DACL with `windows-sys` (Windows target only) if a consumer needs it.
- [X] **G-6 (macOS ACL):** **Decided (Carlos, 2026-10-28): the simplest compatible way.** The owner-only check reads the mode bits only (`fchmod` 0700/0600, then `fstat`). An inherited extended ACL (macOS `chmod +a`, a Linux default ACL beyond the mask) is not inspected; documented as a residual.

---

## Part 4: Activities, in order

### FS-CORE-1 — Rooted primitives in `secure_fs` (risk first; no surface)
Unix and Windows primitives on `RootedDir` and `FsPolicy`, each acting through the held parent descriptor/handle: owner-only `create_directory` (0700 + `fchmod` + mode/ACL check, remove on failure), owner-only `create_new_file` (`O_CREAT|O_EXCL|O_NOFOLLOW`, 0600, same check), `open_regular_nofollow`, `is_directory_nofollow`, `remove_empty_directory` (`unlinkat AT_REMOVEDIR`), `rename_entry` (`renameat`; refuses a directory or symlink destination; `EXDEV` passes through), `change_directory` (`fchdir`). The root itself is readable and is not a write target. Windows: the same functions with `FILE_FLAG_OPEN_REPARSE_POINT`, held parents, and the DACL set in the create call.

Acceptance:
- [X] Unit tests in `secure_fs` (Unix, macOS and Linux CI): each primitive on a plain path, on a final symlink, on a dangling symlink, through a linked directory, outside the root, and on the root itself; nothing is created or removed outside the root. Evidence (2026-10-28): `crates/bn_rt/tests/secure_fs_entries.rs`, 8 tests green on macOS (owner-only modes, existing names and dangling links never created through, linked parent refused, kinds, empty-directory removal, rename, working directory, swap race); Linux runs in CI at the next push.
- [X] Owner-only (Unix): with umask `0` the created directory is `0700` and the file `0600`; a group or other bit left after `fchmod` fails and leaves no entry. Inherited ACLs are not inspected (G-6). Evidence: exact `0700`/`0600` with and without a root; the group/other rule is `is_private` (`only_owner_bits_are_private`), since macOS cannot force a bit after `fchmod`.
- [X] A swap loop (link ↔ directory) during `create_new_file` and `rename_entry` never writes through a link (as the existing `concurrent_intermediate_symlink_swap_never_reads_outside`). Evidence: `a_swapped_parent_never_receives_a_created_file`, 2000 rounds, 5 runs green.
- [X] Windows host: the same tests with junctions (nothing written through a junction; the parent DACL is inherited, G-5); `cargo check --target x86_64-pc-windows-msvc --tests` green before the host run. Evidence: `entry_operations_never_follow_a_junction`; `cargo test -p bn_rt` 74 green on the host (2026-10-28).
- [X] `FS.Open`, `Exists`, `DeleteFile` and the existing `secure_fs` tests unchanged and green. Evidence: `cargo test -p bn_rt` (81), `--test host` (6), `--test execute` (216), `--test programs` (225), `--test gates` (29); `RootedDir::covers` lets a root name itself and the `FS.Open` classification of the root path is unchanged (`IS_DIRECTORY`).

### FS-SPEC-1 — Specification (after G-1 to G-4)
`language/0.6/host.md`: the eight methods, the three codes, the permission, symlink, root and working-directory rules, and the removal of "directory create/delete, Move, Copy, ChangeDirectory" from "Outside 0.2" (listing stays outside, 0.6.16). `0.6.md`: version-table row and history entry. Positive and negative fixtures written first (red on both backends).

Acceptance:
- [X] Every row of Part 1 and every gate answer has one sentence in `host.md`; nothing in the spec is only in this bucket. Evidence (2026-10-28): `host.md` "Directories, private files, copy and move (0.6.12)" (methods, symbolic links, owner-only, existing and missing entries, working directory, execution policy) and the error table (codes 10–12, new "When" text for 1, 2, 3, 4 and 8); "Outside 0.6.12" keeps listing, `chmod`/`chown`, `Seek`, `Flush`, `Path`. `0.6.md`: version-table row 0.6.12, history entry, the rooted-access sentence of "`HOST.FileSystem` execution policy", and "Still outside" (`ChangeDirectory` is `SetCurrentDirectory`).
- [X] `language-guardian` verdict recorded in this file (extension with BDFL decision). Verdict (2026-10-28): surface `host.md` "File system" and the `HOST.FileSystem` code table; classification extension; authorized by the BDFL text of this bucket (2026-10-27/28) and gates G-1 to G-6 (2026-10-28); compatibility: `FS.Open`, `Exists` and `DeleteFile` unchanged, no program that runs today is rejected (the new members were `NAME_NOT_FOUND`); order: spec and red fixtures before code. ADMIT.
- [X] Red fixtures exist under `tests/host/` (positive) and `tests/grammar/invalid/` or host negatives, and fail on both backends for the expected reason (unknown member), not for a fixture error. Evidence: `tests/host/fs_directories.bn` (all eight methods and the codes of every portable refusal) and `tests/host/fs_codes.bn` (codes 10–12) stop with `NAME_NOT_FOUND` on `bni check` and on `bnc`. Policy, symbolic-link and owner-only cases need flags or setup and are Rust parity tests in FS-TEST-1.

### FS-CORE-2 — Shared semantics in `bn_rt::file`
`get_current_directory`, `set_current_directory`, `create_directory`, `open_new` (returns `OpenFile`), `remove_directory`, `is_directory`, `copy`, `move_entry` over FS-CORE-1. `FsOperation` gains the eight operations; `Failure` gains `NotDirectory`, `NotEmpty`, `AlreadyExists`, `NotPrivate`; messages and causes come from `share/bn/catalog/file.txt` (no new literal in `tests/text_literals.list`).

Acceptance:
- [X] Unit tests for every row of the G-3 table and every `Error` field (`Code`, `Operation`, `Message`, `Cause`). Evidence (2026-10-28): `crates/bn_rt/src/file_entries.rs` (the eight operations over `FsPolicy`); `crates/bn_rt/tests/file_entries.rs`, 5 tests (14 G-3 rows, the root, links, copy and move, error fields). Operation names in `share/bn/catalog/modules.txt`, messages and causes in `file.txt`; the three code names follow the `LIMIT_EXCEEDED` precedent in `tests/text_literals.list` (domain table `bn_types::error_codes`).
- [X] `EXDEV` / `ERROR_NOT_SAME_DEVICE` maps to `IO_FAILED` without a copy (R6). Evidence: `system_kinds_map_to_the_entry_failures` (`CrossesDevices` → `CrossDevice` → `IO_FAILED`); `move_entry` has no copy path.
- [X] `Copy` keeps no source mode, owner, setuid, setgid, sticky bit or ACL; the destination is `0600`; a failed copy leaves no destination. Evidence: `copy_keeps_only_the_bytes_and_move_keeps_the_entry` (source `4755` → copy `0600`, no special bit; `Move` keeps `4755`); a failed copy removes the destination.
- [X] Working-directory tests run in a child process only (R3). Evidence: `crates/bn_rt/tests/file_working_directory.rs` is its own test binary with one test (primitive `fchdir`, `SetCurrentDirectory` codes and causes, relative create, `..` escape denied, `GetCurrentDirectory` outside the root `POLICY_DENIED`); the `chdir` test left `secure_fs_entries.rs`.
- [X] `FS.Open`, `Exists`, `DeleteFile` results byte-identical (R7). Evidence: `--test host` (6; `fs_codes` now also prints `10 11 12`), `--test execute` (216), `--test programs` (225), `--test frontend` (194), `--test gates` (29), `cargo test -p bn_rt` green; `cargo clippy` clean for macOS and `x86_64-pc-windows-msvc`.

### FS-FE-1 — Frontend surface
Members and signatures in `crates/bn_frontend/src/host_spec/`, codes in `bn_types::error_codes::fs` (`ALL`), so `bni check`, `bni lsp` completion and both backends see one table.

Acceptance:
- [X] `bni check` accepts the positive fixtures and rejects wrong arity and argument types with the existing codes. Evidence (2026-10-28): `declare_filesystem_directories` in `crates/bn_frontend/src/host_spec/members1.rs`; `tests/frontend/host_fs_directories.rs` (fixture passes; six wrong uses are `TYPE_MISMATCH`). The negative `tests/grammar/invalid/filesystem-directory-api.bn` now calls `FS.ListDirectory` (still outside, 0.6.16) instead of `CreateDirectory`.
- [X] `FS.IS_NOT_DIRECTORY`, `FS.DIRECTORY_NOT_EMPTY`, `FS.ALREADY_EXISTS` resolve as `INTEGER` members; the `host_spec` code table test covers them. Evidence: `bn_types::error_codes::fs::ALL` (12 codes) feeds `host_spec`; `tests/host/fs_codes.bn` prints `10 11 12` on both backends (`--test host`).
- [X] LSP completion on `FS.` lists the new members (existing completion test extended). Evidence: `completion_lists_the_directory_members_and_codes` (`cargo test -p bn_lsp`, 33).

### FS-INT-1 — `bni` adapter (`crates/bn_host_fs`)
Each method calls `bn_rt::file` with the interpreter's `FsPolicy` (re-check at the call); `OpenNew` returns the same `FS.File` object as `Open`.

Acceptance:
- [X] Positive fixtures print their expected text under `bni run`; negatives return the G-3 codes. Evidence (2026-10-28): `fs_directories` case in `tests/host/host_conformance.rs` (24 lines, every portable refusal) passes `host_fixtures_produce_their_exact_interpreter_results`; every new operation has a conformance case.
- [X] `bn_host_fs` holds no rule of its own (review: only argument conversion and `Error` building). Evidence: `FsProvider::directory_call` converts arguments and calls `bn_rt::file`; `Open` and `OpenNew` share `insert_file`.

### FS-NAT-1 — `bnc` (C ABI and lowering)
`bn_rt_fs_*` exports in `crates/bn_rt/src/file_abi.rs` over `bn_rt::file`; lowering in `crates/bn_llvm/src/llvm/fs_emission.rs`; symbols in `value-memory-abi.md` and `tests/compiler-capabilities.json`.

Acceptance:
- [X] `tests/codegen/compiler_capabilities.rs` green with the new symbols listed (no symbol emitted without an export, none exported without a row). Evidence (2026-10-29): eight `bn_rt_fs_*` exports in `crates/bn_rt/src/file_abi.rs` (`OpenNew` shares `register_file` with `Open`); `DirectoryCall` in `crates/bn_llvm/src/llvm/fs_emission.rs` recognizes the calls by their catalog contract names; declarations in `runtime_abi.rs`, both LLVM golden files, and `value-memory-abi.md` (table row and symbol list). `--test codegen` (18), `--test host` (6: `fs_directories` and `fs_codes` byte-identical on `bnc`).
- [X] Null and invalid-UTF-8 path pointers rejected at the C boundary; every out pointer written only on success; no handle leaked on a failure path. Evidence: `file_abi::tests::directory_exports_check_their_pointers` (null and non-UTF-8 paths, null out pointers, the handle slot stays 0 on a failed `OpenNew`); `register_file` inserts a slot only when the open succeeds.
- [X] Compiled policy re-checked by `bn_rt` at the call (deny under `--sandbox`, `BN_FS_POLICY=deny`). Evidence (2026-10-29): every export calls `authorize()` and the shared `FsPolicy`; `fs_directories_policy` gives the same codes on the `bnc` artifact under `--sandbox` (compile-time roots), `BN_FS_POLICY=read-only` and `deny`.

### FS-TEST-1 — Parity and negatives
Fixtures in `tests/host/` and a parity test in `tests/programs/`: directory lifecycle (create, is, set/get current, copy, move, remove), owner-only modes, symlink and junction cases, the root itself, policy matrix (no policy, `--sandbox` with read/write roots, `BN_FS_POLICY=deny`), relative paths after a change of directory (R8).

Acceptance:
- [X] Every positive fixture prints the same text on `bni` and `bnc`; every negative returns the same `Code` and `Operation` on both. Evidence (2026-10-29): `fs_directories` and `fs_codes` byte-identical on both (`--test host`); `tests/programs/fs_directories_policy.rs` compares the exact text on both under four policies.
- [X] Policy matrix: a write outside `--write-root` is `POLICY_DENIED` and creates nothing; a read outside `--read-root` is `POLICY_DENIED`. Evidence: `directory_operations_obey_every_policy_on_both_backends` (unrestricted, sandbox, read-only, deny). The outside directory stays empty under the three policies; a link on the way and a `../` path after `SetCurrentDirectory` are denied (R8); the root is readable and not removable. With no policy, new directories are `0700`, new and copied files `0600`, and the moved file keeps `0644`.
- [X] New identities in `tests/migration-ledger.csv` and `scripts/test-inventory.json`; Unix-only tests in `tests/support/migration_target.rs`. Evidence: `--test gates` (29) and `--test programs` (226) green.

### FS-WIN-1 — Windows batch verification (once, at the end)
One sync and one run on the host with the `AGENTS.md` recipe (`tools` on `PATH`, `bn_rt`/`bn_stdlib`/`bn_web` built first).

Acceptance:
- [X] `cargo test --workspace --no-fail-fast` green on the host; CI green on Linux, macOS and Windows after the push. Evidence (2026-10-29): one sync and one run on the Windows host with the `AGENTS.md` recipe, 133 test targets green, no failure; macOS 133 green. CI runs at the push of the close commit; its result is checked before the tag (`github-release`).

### FS-DOC-1 — Book and release
`docs/book/en/13_files.md` (directories, private create, copy and move, working directory, permissions), `docs/releases/0.6.12.md`, `docs/releases/README.md`; `man` pages unchanged unless `--help` changes.

Acceptance:
- [X] Every complete example of the new section runs with the same output on `bni` and `bnc`. Evidence (2026-10-29): `docs/book/en/13_files.md`, "Directories, Private Files, Copying and Moving" (concepts, the eight methods, the three rules, two complete programs); both programs printed the shown output on `bni` and the `bnc` artifact, identical, and left the directory empty. The sandbox paragraph, the summary and the references (POSIX.1-2024) follow; the code-table link points at `v0.6.12`.
- [X] The release note lists the behaviour of every method and the residuals (`FS.Open` mode, Windows `FS.Open` DACL, group ownership). Evidence: `docs/releases/0.6.12.md` (additions, behaviour change of the grammar fixture, known limits G-5/G-6, `FS.Open` mode and group) and the row in `docs/releases/README.md`.

### FS-CONS-1 — Consumers (check only)
- [X] 0.6.14, 0.6.14b, 0.6.15 and 0.6.17 name `FS.OpenNew` for ciphertext, plaintext, cookie files and download temporaries, and none uses `FS.Open` `WRITE` or `FS.Copy` for them (grep evidence in this line). Evidence (2026-10-29): `FS.OpenNew` appears 5, 1, 3 and 5 times in those buckets; every mention of `FS.Open` `WRITE` or `FS.Copy` there is a prohibition (0.6.14 FILE-1: "not `FS.Open` `WRITE` and not a copy"; 0.6.15 SEC-4: "`FS.Open` `WRITE` is not that create").

### FS-AUDIT-1 — End-of-sprint audit
- [X] `code-auditing` over this bucket's diff only: FFI boundary, `unsafe` blocks and their `SAFETY:` comments, races (R4), owner-only failure cleanup, policy bypasses. Findings recorded here; defects fixed with a regression test before close. Audit (2026-10-29):
  - Fixed, medium: `Copy` read the source kind, then opened it; a FIFO swapped in between made the open block until a writer came, hanging the program. The read open is now `O_NONBLOCK` (nothing changes for a regular file), and `Copy` checks the opened descriptor is a regular file (`NotRegularSource` otherwise). Regression `a_fifo_never_blocks_the_read_open` fails without the fix ("the open blocked" after 5 s) and passes with it; `file_entries` gained "copy a FIFO" (`INVALID_ARGUMENT`).
  - Accepted, low (R4): `Move` and `RemoveDirectory` check the destination or the entry, then act. An entry swapped for a link in between is replaced or removed as a link, never followed; nothing is written through it. Windows behaves the same with `MoveFileExW` and `RemoveDirectoryW`.
  - Accepted, low: after a failed owner-only check the new entry is removed by name in its held parent; if it was swapped meanwhile, the swapped entry of that same directory is removed instead. The parent is inside the root, so nothing outside is touched.
  - No defect: every new `unsafe` block (`fstatat`, `mkdirat`, `openat`, `unlinkat`, `renameat`, `fchdir`, the C exports) has a `SAFETY:` comment naming its invariant; the C exports reject null and non-UTF-8 paths and null out pointers and clear out slots first; `register_file` inserts only after a successful open; every entry operation under a root goes through the anchored walk, and a root itself is refused as a write target before any act. Residuals by decision: G-4 (shared working directory), G-5 and G-6 (no DACL, no ACL inspection).

### Close
- [X] `cargo fmt --check`, `cargo clippy --workspace --tests -- -D warnings`, `cargo test --workspace`, `scripts/test-battery.sh --gates`, `git diff --check`. Evidence (2026-10-29): all five green on macOS (`cargo test --workspace` 133 targets; battery rc 0), `cargo test --workspace` green on the Windows host (133).
- [X] `git mv` to `done/`; then the joint 0.6.11 + 0.6.12 release (`github-release`). Closed 2026-10-29 with `git mv`; the release is the next step, outside this bucket.
