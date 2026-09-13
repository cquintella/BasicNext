# F6 — RELEASE primary

**Status:** `RED_EXPECTED_UNTIL_M2` — fixture committed before M2/D1 runtime (Carlos/Quorra fixtures-first).  
**Backend:** interpret (`bn run`)  
**Command:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F6/program.bn
```
**Expected exit (when M2/D1 green):** `non-zero`  
**Expected observation:** use-after-release on primary; no refcount story required

**Today (pre-M2):** may fail parse/typecheck/runtime on 0.4 toolchain — that is intentional red until waves land.  
**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
