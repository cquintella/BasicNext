# F12 — No DELETE in fixture program + scope lifetime

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)

Split checks (do **not** combine with a broken `rg && bn run` pipeline — zero-match `rg` exits 1):

### (a) No `DELETE` token in `program.bn` only

```bash
! rg -n '\bDELETE\b' docs/superpowers/evidence/arc-0.5.0/F12/program.bn
```

Expected: no matches (shell `!` inverts; plain `rg` exit 1 on zero matches is success for absence).  
Scope: **`program.bn` only** — not the whole `arc-0.5.0/` tree.

### (b) Scope / lifetime run

```bash
bn run docs/superpowers/evidence/arc-0.5.0/F12/program.bn
```

**Expected exit (when M2 green):** `0`  
**Expected observation:** program prints `LIVE` then `DEINIT` (runtime lifetime).

### (c) NOTES mentions OK

This `NOTES.md` file **may** mention the word DELETE when documenting the forbidden surface. Mentions here are **not** a fixture failure. Do not scan NOTES (or other Markdown) as if they were program tokens.

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — intentional red until waves land.  
**Forbidden in `program.bn`:** `DELETE`, force-dispose, silent use-after-release.
