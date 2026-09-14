# F4 — Cycle + weak → NULL

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F4/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F4/program.bn -o /tmp/F4 && /tmp/F4
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `PASS F4 weak NULL`
**Observed (native):** exit `0`; `PASS F4 weak NULL`

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
