# F3 — Reassign drop

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F3/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F3/program.bn -o /tmp/F3 && /tmp/F3
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `DEINIT  1` then `HELD  2` then `DEINIT  2`
**Observed (native):** exit `0`; DEINIT on reassign + scope end

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
