# F12 — No DELETE in 0.5.0 fixtures

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
rg -n '\\bDELETE\\b' docs/superpowers/evidence/arc-0.5.0 && bn run docs/superpowers/evidence/arc-0.5.0/F12/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** rg finds zero DELETE under arc-0.5.0/; program prints LIVE then DEINIT; this file has no DELETE token

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
