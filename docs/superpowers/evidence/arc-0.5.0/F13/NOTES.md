# F13 — Typed AWAIT + args

**Status:** `RED_EXPECTED_UNTIL_D1` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F13/program.bn
```
**Expected exit (when M2/D1 green):** `0`  
**Expected observation:** PASS F13 sum=10 (1+2+3+4); AWAIT yields INTEGER OR Error

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
