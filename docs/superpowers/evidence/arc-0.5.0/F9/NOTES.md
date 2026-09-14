# F9 — No RELEASE a[i] as remove-middle

**Status:** `GREEN_REJECT` interpret + `GREEN_REJECT` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`) — rejected in frontend validation before emit

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F9/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F9/program.bn -o /tmp/F9 && /tmp/F9
```

**Expected exit:** `1` (both; build fails, no native artifact)
**Observed (interpret):** exit `1`; `INVALID_RELEASE_TARGET` (indexed RELEASE rejected)
**Observed (native/build):** exit `1`; same `INVALID_RELEASE_TARGET` diagnostic (not `TARGET_UNSUPPORTED_OP`)

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
