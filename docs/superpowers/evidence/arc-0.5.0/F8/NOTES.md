# F8 — RELEASE vector aggregate

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F8/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F8/program.bn -o /tmp/F8 && /tmp/F8
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `BOX_DEINIT` x2 then `AFTER_RELEASE_VECTOR`
**Observed (native):** exit `0`; `BOX_DEINIT` aggregate release

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
