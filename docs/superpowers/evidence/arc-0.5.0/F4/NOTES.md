# F4 — Cycle + weak → NULL

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F4/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** PASS F4 weak NULL after last strong to cycled nodes dropped

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
