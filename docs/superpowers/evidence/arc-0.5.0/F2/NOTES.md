# F2 — Scope deinit

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F2/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** Order: IN_SCOPE, DEINIT (once), AFTER_SCOPE — no double DEINIT

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
