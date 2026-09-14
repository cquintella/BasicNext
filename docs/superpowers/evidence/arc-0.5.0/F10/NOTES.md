# F10 — RELEASE object drop-one-strong

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F10/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F10/program.bn -o /tmp/F10 && /tmp/F10
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `PASS F10 survivor n= 3` then final `DEINIT`
**Observed (native):** exit `0`; survivor PASS + one final DEINIT

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
