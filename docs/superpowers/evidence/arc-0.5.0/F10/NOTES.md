# F10 — RELEASE object drop-one-strong

**Status:** `RED_EXPECTED` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F10/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** PASS F10; no DEINIT before end while b lives; DEINIT only when b leaves scope

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
