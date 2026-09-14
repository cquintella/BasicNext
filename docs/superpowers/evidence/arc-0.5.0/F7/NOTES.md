# F7 — RELEASE struct

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F7/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F7/program.bn -o /tmp/F7 && /tmp/F7
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `INNER_DEINIT` / `AFTER_RELEASE_STRUCT` (`UNUSED_BINDING` warn OK)
**Observed (native):** exit `0`; same observations

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
