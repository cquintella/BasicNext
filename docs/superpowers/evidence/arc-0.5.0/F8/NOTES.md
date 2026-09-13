# F8 — RELEASE vector aggregate

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F8/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** BOX_DEINIT x2 (order unspecified) then AFTER_RELEASE_VECTOR

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
