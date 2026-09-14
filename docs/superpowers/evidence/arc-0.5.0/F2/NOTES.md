# F2 — Scope deinit

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F2/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F2/program.bn -o /tmp/F2 && /tmp/F2
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `IN_SCOPE` / `DEINIT` / `AFTER_SCOPE`; `UNUSED_BINDING` warn OK
**Observed (native):** exit `0`; same order

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
