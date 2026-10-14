# Proposal: HOST.FileSystem directory and metadata ops (0.5.3)

**Status:** Proposed — expands existing capability (not a new module).
**Date:** 2026-09-19
**Target:** **0.5.3** (with S1 BNSqlite + A1 BNWeb Auth).
**Gate:** Quorra before Carlos.
**Motivation (Carlos 2026-09-19):** Beside SQLite, programs need directory work, listing, delete, attributes/metadata, move, and copy. Selecting a directory via a GUI picker is **separate** (see Non-goals / Open).

**Baseline:** [`docs/library/host.md`](../../docs/library/host.md) — today: `Open` / read-write / `Exists` / `DeleteFile`. Explicitly **outside 0.2**: directory create/list/delete, `Move`, `Copy`, `chmod`/`chown`, `Seek`, `Flush`, `Path` object.

**Policy:** Stay on **`IMPORT HOST.FileSystem AS FS`**. Do not invent `BNFiles`. Paths remain `STRING` (UTF-8). Fail-closed (`T OR Error`). ARC: file handles use `Close` / `RELEASE` (no `DELETE` keyword). Capability deny → `HOST_CAPABILITY_UNAVAILABLE` / policy Error.

---

## Goals (claim candidates)

| ID | Item | Spirit |
| --- | --- | --- |
| **FS1** | Directory create / remove | `CreateDirectory(path)`, `RemoveDirectory(path)` — empty-dir remove; missing/non-empty → Error (lock recursive delete as **out** or explicit `RemoveDirectoryRecursive` if Carlos expands) |
| **FS2** | List | `List(path) AS Data.DataFrame OR Error` **or** `VECTOR OF FS.Entry` — name, kind (file/dir/symlink), size, mtime; lock shape in § API |
| **FS3** | Copy / Move | `Copy(src, dst)`, `Move(src, dst)` — overwrite policy locked (fail vs replace) |
| **FS4** | Delete file | Already `DeleteFile`; confirm parity + docs; optional `Delete` alias **not** required |
| **FS5** | Metadata / attributes | `Stat(path) AS FS.Stat OR Error` (size, mtime, ctime if portable, mode bits, is_dir/is_file); `SetMode(path, mode)` where OS allows; **no** portable `chown` claim unless Unix-only row in support matrix |
| **FS6** | Working directory (optional) | `GetWorkingDirectory` / `ChangeDirectory` — useful for tools; sandbox policy must re-check roots |

## Non-goals (0.5.3 unless Carlos expands)

- **GUI “selecionar pasta”** (`PickDirectory` / folder dialog) — that is **HOST.Ui** (or a tiny OS-dialog capability). 0.5.3 FS is **path-addressed**. A later `FS.PickDirectory() AS STRING OR Error` may wrap the OS dialog **without** full BNUI, but only if Carlos locks it as an explicit FS7 row.
- Watchers / notify (`inotify`), mmap, file locks, hard/soft link create (symlink **read** via Stat kind may be enough).
- Full `Path` object API (join/normalize) — may ship thin `Join(a,b) AS STRING` helper if needed for teaching; rich Path = follow-on.
- Recursive tree delete by default.
- Cross-volume Move semantics beyond “OS rename or Error”.

## Suggested API sketch (illustrative)

```basic
IMPORT HOST.FileSystem AS FS
IMPORT BNData AS Data

FUNCTION Start() AS VOID
  LET made AS VOID OR Error = FS.CreateDirectory("out")
  IF made IS Error THEN RETURN

  LET entries AS Data.DataFrame OR Error = FS.List(".")
  IF entries IS Error THEN RETURN

  LET st AS FS.Stat OR Error = FS.Stat("out")
  IF st IS Error THEN RETURN

  LET copied AS VOID OR Error = FS.Copy("a.txt", "out/a.txt")
  LET moved AS VOID OR Error = FS.Move("out/a.txt", "out/b.txt")
  LET gone AS VOID OR Error = FS.DeleteFile("out/b.txt")
END FUNCTION
```

Exact `Stat` fields and `List` return type (DataFrame vs Entry vector) freeze in bucket § lock before fixtures.

## Relation to BNSqlite

SQLite opens **paths**. Directory create + Exists + Copy of `.db` files are natural companions; do not embed filesystem into `BNSqlite`.

## Open questions

1. `List` → DataFrame vs `VECTOR OF Entry`?
2. Overwrite on Copy/Move: fail-closed always, or flag?
3. Symlinks: follow vs not (Stat vs LStat)?
4. Include **FS7** PickDirectory in 0.5.3 or defer to Ui?
5. Windows vs Unix mode bits — support-matrix honesty.

## Acceptance (no done-oco)

Fixtures for create/list/stat/copy/move/delete (+ fail-closed). Interpret green; native parity if claimed. Book/library updated. Quorra gate before tag.

**Retarget (2026-09-21, invert):** claim lives in `todo/bucket-0.6.3.md` (was 0.5.3).
