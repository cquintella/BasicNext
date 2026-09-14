# F12 — No DELETE in 0.5.0 fixtures

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F12/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F12/program.bn -o /tmp/F12 && /tmp/F12
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `LIVE` then `DEINIT` (`UNUSED_BINDING` warn OK); fixture tree has no `DELETE` keyword
**Observed (native):** exit `0`; `LIVE` then `DEINIT` (same order on this tree; if DEINIT order differs, record actual LIVE-only vs LIVE+DEINIT)

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
