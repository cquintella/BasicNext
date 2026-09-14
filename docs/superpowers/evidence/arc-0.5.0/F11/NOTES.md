# F11 — Tickets Close + RELEASE aggregate

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F11/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F11/program.bn -o /tmp/F11 && /tmp/F11
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `PASS F11 tickets Close+RELEASE`
**Observed (native):** exit `0`; `PASS F11 tickets Close+RELEASE`

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
