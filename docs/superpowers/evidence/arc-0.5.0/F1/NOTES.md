# F1 — Strong aliasing

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F1/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** PASS F1; both aliases see n=7; no premature deinit

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
