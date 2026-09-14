# F1 — Strong aliasing

**Status:** `GREEN` interpret + `GREEN` native (Tron 2026-09-13).
**Backend:** interpret (`bn run`) and LLVM/native (`bn build`)

**Commands:**
```bash
bn run docs/superpowers/evidence/arc-0.5.0/F1/program.bn
bn build docs/superpowers/evidence/arc-0.5.0/F1/program.bn -o /tmp/F1 && /tmp/F1
```

**Expected exit:** `0` (both)
**Observed (interpret):** exit `0`; `PASS F1 strong alias n= 7`
**Observed (native):** exit `0`; `PASS F1 strong alias n= 7`

**Forbidden:** `DELETE`, force-dispose, silent use-after-release.
