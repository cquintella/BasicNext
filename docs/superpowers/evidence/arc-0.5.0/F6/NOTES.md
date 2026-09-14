# F6 — RELEASE primary

**Status:** `GREEN_EXPECTED_TRAP` interpret + `GREEN_EXPECTED_TRAP` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F6/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F6/program.bn -o /tmp/F6 && /tmp/F6
```

**Expected exit:** `1` (both)
**Observed (interpret):** exit `1`; `USE_AFTER_RELEASE: binding was released`
**Observed (native):** exit `1`; `USE_AFTER_RELEASE: binding was released`

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
